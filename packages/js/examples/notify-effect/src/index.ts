import { defineEffect, defineEvent, exportEffect, env } from "@umari/js";

type UserRegisteredData = {
  userId: bigint;
  email: string;
};

const UserRegistered = defineEvent<UserRegisteredData>()("user.registered", {
  domainIds: ["userId"],
});

const NotifyUser = defineEffect({
  events: [UserRegistered],
  init: () => ({ endpoint: env("NOTIFY_ENDPOINT", "https://example.com/notify") }),
  partitionKey: (event) => event.data.userId.toString(),
  handle: async (event, state) => {
    const r = await fetch(state.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ userId: event.data.userId.toString(), email: event.data.email }),
    });
    if (!r.ok) throw new Error(`status ${r.status}`);
  },
});

export const { effect } = exportEffect(NotifyUser);
