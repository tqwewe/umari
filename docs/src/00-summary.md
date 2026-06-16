# The Umari Book

**Status**: Draft.

This book is the official guide to building event-sourced systems with Umari. It covers concepts, patterns, API reference, and operations — everything you need to go from zero to production.

## Who this book is for

You should be comfortable with either **Rust** or **TypeScript** — Umari modules can be written in both, and every code example in this book is shown in both. Prior event sourcing experience helps but is not required; the first part explains the concepts from scratch.

## How to read this book

- **Part 1 — Concepts**: Read straight through. These chapters establish the mental model.
- **Part 2 — Building Blocks**: Read straight through. You'll use every concept here.
- **Part 3 — Module Types**: Reference each chapter as you build that module type.
- **Part 4 — Working with Umari**: Practical patterns, project structure, and API reference.
- **Part 5 — Runtime & Operations**: Read when deploying or debugging.

Code examples follow current API conventions and appear in language tabs — pick Rust or TypeScript once and every snippet in the book follows your choice.

## Conventions

- **SDKs**: `umari` refers to the Rust SDK crate (`crates/umari`); `@umari/js` is the TypeScript SDK. `umari-runtime` refers to the runtime. Module crates/packages use kebab-case names.
- **Language tabs**: Snippets that differ by language are shown in **Rust** / **TypeScript** tabs. The two SDKs share one WIT contract and produce interchangeable WASM modules.
