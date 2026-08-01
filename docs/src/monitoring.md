# Monitoring & Alerting

Umari exposes Prometheus metrics so you can watch the runtime in production and get paged when a projector or effect stops running or falls behind. The metrics are pull-based (you scrape them), scoped to **liveness and lag**, and designed around one goal: catching silent failures before they become data problems.

This chapter covers the `/metrics` endpoint, the metrics it exports, a ready-made [vmui](https://docs.victoriametrics.com/victoriametrics/#vmui) dashboard, and [vmalert](https://docs.victoriametrics.com/victoriametrics/vmalert/) rules that notify you over Discord.

## The `/metrics` endpoint

The runtime serves metrics in Prometheus text format at:

```
GET /metrics
```

### Authentication

`/metrics` is part of the authenticated API surface. When `UMARI_API_KEY` is set (as it should be in production), scrapers **must** send the bearer token:

```sh
curl -H "Authorization: Bearer $UMARI_API_KEY" http://localhost:3000/metrics
```

Without the header the endpoint returns `401`. Keep this in mind when configuring your scraper; see [Scraping](#scraping-with-victoriametrics--prometheus) below.

### Collection interval

Two kinds of metrics are exported:

- **Event-driven** metrics (failures, restarts, backoff, progress timestamps) are recorded inline as things happen and are always available.
- **State-derived** gauges (up, positions, lag) are refreshed by a periodic collector.

Control the collector with:

| Flag | Env | Default | Description |
|------|-----|---------|-------------|
| `--metrics-interval` | `UMARI_METRICS_INTERVAL` | `15s` | How often to refresh state-derived metrics. `0` disables the collector entirely (event-driven metrics are still recorded). |

## Metrics reference

Every metric is labelled with `module_type` (`projector`, `effect`, or `command`) and `name` (the module name). `umari_module_info` additionally carries `version`.

| Metric | Type | Meaning |
|--------|------|---------|
| `umari_module_up` | gauge | `1` if the module is alive and healthy, `0` if it has died. Deactivated modules stop reporting rather than going to `0`. |
| `umari_module_info` | gauge | Always `1`; carries the running `version` as a label. |
| `umari_module_last_position` | gauge | The module's committed global event-store position. |
| `umari_module_query_head_position` | gauge | Global position of the latest event matching this module's own query. |
| `umari_module_lag` | gauge | Events behind: `query_head - last_position`, clamped to `0`. `0` means caught up. |
| `umari_module_last_progress_timestamp_seconds` | gauge | Unix time of the module's last committed progress. `time() - this` is staleness. |
| `umari_module_failures_total` | counter | Number of times the module actor has died. |
| `umari_module_restarts_total` | counter | Number of times the module has been restarted (effects only). |
| `umari_module_backoff_seconds` | gauge | Current restart backoff delay; `0` when healthy. |
| `umari_event_store_head_position` | gauge | Global head position of the event store (informational). |

Two design facts shape how these are used:

- **Lag is query-aware.** A projector or effect only subscribes to the events matching its query. A narrowly-subscribed module that is fully caught up legitimately sits far below the global event-store head, so lag is measured against each module's *own* query head (`umari_module_query_head_position`), not the global head. This avoids false lag. Alert on `umari_module_lag`, never on `head - last_position`.
- **Projectors and effects fail differently.** Projectors do **not** auto-restart: a dead projector stays down (`umari_module_up == 0`) until reactivated, which is the primary silent-failure case. Effects **do** auto-restart with exponential backoff, so a crash surfaces as climbing `umari_module_restarts_total` and `umari_module_backoff_seconds` rather than a permanent down state.

## Scraping with VictoriaMetrics / Prometheus

Point your scraper at `/metrics` and pass the API key as a bearer token. A [vmagent](https://docs.victoriametrics.com/victoriametrics/vmagent/) or Prometheus scrape config looks like:

```yaml
scrape_configs:
  - job_name: umari
    scheme: https
    metrics_path: /metrics
    authorization:
      type: Bearer
      credentials: "your-umari-api-key"
    static_configs:
      - targets: ['ops.example.com']
```

vmalert (below) queries VictoriaMetrics, not the runtime directly, so the bearer token only needs to live in the scrape config.

## Dashboard (vmui)

A prebuilt dashboard for vmui lives at [`docs/monitoring/umari-runtime.json`](https://github.com/tqwewe/umari/blob/main/docs/monitoring/umari-runtime.json). Load it by pointing VictoriaMetrics at the directory containing it:

```sh
victoria-metrics --vmui.customDashboardsPath=/path/to/dashboards
```

It appears under the **Dashboards** tab in vmui. The dashboard has five rows:

1. **Health** — modules currently down (`umari_module_up == 0`), and active/down counts per type.
2. **Lag & freshness** — modules behind their query head, and time since last progress.
3. **Failures & restarts** — recent failures, effect restarts, and backoff.
4. **Event store & throughput** — ingestion rate, head position, per-module progress rate.
5. **Deployments** — modules per running version, so you can correlate incidents with rollouts.

The panels follow a "plot problems, not inventory" approach: filters like `== 0` and `> 0` mean most panels are empty when everything is healthy and light up with exactly the offending module when something breaks. This also keeps them under vmui's per-panel series limit.

Two things to know:

- **vmui renders only line graphs.** There is no stat, gauge, or table panel type. For those, use Grafana with VictoriaMetrics as a Prometheus data source.
- **The overview panels assume a single runtime instance.** Aggregations like `sum(...) by (module_type)` drop the scraper's `instance` label. If you run multiple runtime instances, add `instance` to the `by(...)` clauses so they don't merge.

## Alerting with vmalert + Discord

[vmalert](https://docs.victoriametrics.com/victoriametrics/vmalert/) evaluates alerting rules against VictoriaMetrics and sends firing alerts to [Alertmanager](https://github.com/prometheus/alertmanager), which delivers them to Discord via its native receiver.

### Alerting rules

Save these as `umari-alerts.yml` and pass them to vmalert with `-rule`:

```yaml
{{#include ../monitoring/umari-alerts.yml}}
```

The rules map directly onto the failure model described above:

- **`UmariRuntimeUnreachable`** — the runtime or its `/metrics` endpoint stopped reporting entirely.
- **`UmariProjectorDown`** — a projector has been down for 2m. Projectors don't self-heal, so this is critical.
- **`UmariEffectNotRecovering`** — an effect has been down longer than its max backoff, so it is genuinely stuck.
- **`UmariEffectCrashLooping`** — an effect keeps restarting.
- **`UmariModuleLagging`** / **`UmariModuleStalled`** — a module is behind its query head, or behind *and* making no progress.
- **`UmariModuleFlapping`** — a module recovers but keeps dying.

The thresholds (lag `> 1000`, restart/failure counts) are starting points — tune them to your event volume.

### Alertmanager Discord receiver

Alertmanager has a native Discord receiver (v0.25.0+), so no bridge is needed. Save this as `alertmanager.yml` and replace the webhook placeholder with your channel's webhook URL (Discord: **Channel → Edit → Integrations → Webhooks → New Webhook → Copy URL**):

```yaml
{{#include ../monitoring/alertmanager.yml}}
```

### Wiring it together

Run Alertmanager and vmalert alongside VictoriaMetrics. A docker-compose sketch:

```yaml
services:
  alertmanager:
    image: prom/alertmanager:latest
    command: ['--config.file=/etc/alertmanager/alertmanager.yml']
    volumes:
      - ./monitoring/alertmanager.yml:/etc/alertmanager/alertmanager.yml:ro

  vmalert:
    image: victoriametrics/vmalert:latest
    command:
      - -rule=/etc/vmalert/umari-alerts.yml
      - -datasource.url=http://victoriametrics:8428   # VM that scrapes umari
      - -notifier.url=http://alertmanager:9093
      - -remoteWrite.url=http://victoriametrics:8428   # persist alert state across restarts
      - -remoteRead.url=http://victoriametrics:8428    # restore it on boot
    volumes:
      - ./monitoring/umari-alerts.yml:/etc/vmalert/umari-alerts.yml:ro
```

`-remoteWrite`/`-remoteRead` are optional but recommended: without them a vmalert restart drops all in-flight alert `for:` timers.

### Using Matrix instead

Alertmanager has no native Matrix receiver. To deliver to Matrix, run a bridge — [matrix-hookshot](https://matrix-org.github.io/matrix-hookshot/latest/setup/webhooks.html) with a generic webhook, or the [matrix-alertmanager](https://github.com/jaywink/matrix-alertmanager) relay — and point a `webhook_configs` receiver at it instead of `discord_configs`. The alerting rules stay identical.
