# The Umari Book

This book is the official guide to building event-sourced systems with Umari. It covers concepts, patterns, API reference, and operations: everything you need to go from zero to production.

## Who this book is for

You should be comfortable with either **Rust** or **TypeScript**. Umari modules can be written in either, and every code example in this book is shown in both languages. Prior event sourcing experience helps but is not required; the opening chapters explain the concepts from scratch.

## How to read this book

- **Concepts**: Read straight through. These chapters establish the mental model.
- **Building Blocks**: Read straight through. You'll use every concept here.
- **Module Types**: Reference each chapter as you build that module type.
- **Working with Umari**: Practical patterns, project structure, and API reference.
- **Runtime & Operations**: Read when deploying or debugging.

Code examples follow current API conventions and appear in language tabs: pick Rust or TypeScript once and every snippet in the book follows your choice.

## Conventions

- **SDKs**: `umari` refers to the Rust SDK crate (`crates/umari`); `@umari/js` is the TypeScript SDK. `umari-runtime` refers to the runtime. Module crates/packages use kebab-case names.
- **Language tabs**: Snippets that differ by language are shown in **Rust** / **TypeScript** tabs. The two SDKs share one WIT contract and produce interchangeable WASM modules.
