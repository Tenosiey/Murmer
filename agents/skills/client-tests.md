# Client tests

Read this before writing a Vitest test in `murmer_client/`.

The suite map and the selection rule are in
[`../../docs/testing.md`](../../docs/testing.md). This is how to write one.

## Does it deserve a test?

Test the logic whose failure is **invisible in the UI**. Concretely: state
namespaced per server URL, request/response correlation, parsing of untrusted
server frames, policy that only manifests later, and edge cases in pure
algorithms.

**Do not add component-rendering tests.** A broken layout is caught by
looking at the app ([`visual-verification.md`](visual-verification.md)), and
a rendering test buys assertion cost without buying confidence.

## Where the file goes

Next to the module it covers, as `src/lib/**/*.test.ts`. Only shared harness
code and the mirror test live in `test/`.

## The store pattern

Stores are module-level singletons that wire themselves up on import, so a
test that needs a clean one takes a fresh instance:

```ts
beforeEach(async () => {
  vi.resetModules();
  const store = await import('$lib/stores/example');
  // ...
});
```

**Mock `./chat`** rather than pulling in the `WebSocketManager`. If the
module under test is a leaf that sends frames, it should be taking its
transport by injection (`setTransport`) — that is what makes it testable at
all, and if it is not, fixing that is the better change.

`localStorage` is a real in-memory `Storage` provided by `test/setup.ts` and
cleared before every test, so persisted state cannot leak between them.
`$app/environment` is stubbed with `browser = true`, because the app only
ever runs in a browser and a `false` there would silently skip every
persistence path under test.

## The environment

Tests run in plain **Node**. Two suites opt into jsdom with a
`// @vitest-environment jsdom` docblock because they cover the message-HTML
boundary and need a DOM to parse into — `markdown.test.ts` and
`emoji.test.ts`.

If you need a DOM, use **jsdom, not happy-dom**. happy-dom 20 mis-drives
DOMPurify's tree walk and leaves `<script>` tags in the output, so a suite
written against it would assert the exact opposite of the truth.

Only `$lib` and `$app/environment` are aliased. If a test needs a third piece
of the framework, that is usually a sign the logic should move into a pure
module first.

## Writing the assertions

- **Assert the invariant the test is named for**, not a whole structure
  snapshot, so unrelated churn does not fail it.
- **Cover the denial, not just the happy path** — the "key changed, refuse to
  wrap" case is the one that matters.
- **Open the file with a comment saying why the area is worth testing.**
  `test/server-mirror.test.ts` is the model.

## Running

```bash
cd murmer_client && bun run test
```

`bun run test:watch` while iterating. `bun run check` must also pass with 0
errors and 0 warnings.
