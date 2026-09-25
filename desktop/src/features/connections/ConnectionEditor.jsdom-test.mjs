import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { ConnectionEditor } from "./ConnectionEditor.tsx";

const calls = [];
let handler = () => Promise.reject(new Error("Unexpected IPC"));
const bridge = {
  invoke(command, args) {
    calls.push({ command, args });
    return Promise.resolve(handler(command, args));
  },
  transformCallback() {
    return 1;
  },
};
globalThis.__TAURI_INTERNALS__ = bridge;
window.__TAURI_INTERNALS__ = bridge;
window.matchMedia ??= () => ({
  matches: false,
  addListener() {},
  removeListener() {},
  addEventListener() {},
  removeEventListener() {},
});
let root, container;
afterEach(async () => {
  if (root) await act(async () => root.unmount());
  container?.remove();
  root = null;
  calls.length = 0;
});
const connection = () => ({
  id: crypto.randomUUID(),
  name: "Development server",
  revision: 0,
  target: {
    type: "ssh",
    endpoint: {
      mode: "manual",
      host: "server.test",
      port: 22,
      username: "engineer",
      authentication: { method: "password" },
    },
  },
  defaults: { harness_id: null, model: null },
  check: null,
});
async function mount(props) {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root.render(React.createElement(ConnectionEditor, props)),
  );
}
async function click(label) {
  const button = [...container.querySelectorAll("button")].find(
    (button) => button.textContent === label,
  );
  assert.ok(button, `button: ${label}`);
  await act(async () => button.click());
}

test("saving an unchecked SSH connection does not test or start it", async () => {
  const draft = connection();
  let saved;
  handler = (command, args) => {
    assert.equal(command, "save_execution_connection");
    return { ...args.connection, revision: 1 };
  };
  await mount({
    initial: draft,
    onSaved: (value) => {
      saved = value;
    },
    onCancel() {},
  });
  await click("Save connection");
  assert.equal(saved.name, draft.name);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["save_execution_connection"],
  );
  assert.equal(calls[0].args.credentials, null);
  assert.equal(calls[0].args.checkTicket, null);
});

test("testing and cancelling preserve the draft without saving", async () => {
  const draft = connection();
  let cancelled = false;
  handler = (command) => {
    assert.equal(command, "test_execution_connection");
    return {
      check: {
        revision: 0,
        checked_at: 1,
        outcome: { status: "authentication_required" },
        server_id: null,
        default_directory: null,
      },
      harnesses: [],
      trust_prompt: null,
      ticket: "check-ticket",
    };
  };
  await mount({
    initial: draft,
    onSaved() {
      assert.fail("Test must not save");
    },
    onCancel() {
      cancelled = true;
    },
  });
  await click("Test connection");
  assert.match(container.textContent, /Sign-in failed/);
  assert.ok(
    [...container.querySelectorAll("input")].some(
      (input) => input.value === draft.name,
    ),
  );
  await click("Cancel");
  assert.equal(cancelled, true);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["test_execution_connection"],
  );
});

test("an unknown server requires an explicit trust action", async () => {
  const draft = connection();
  handler = (command) => {
    assert.equal(command, "test_execution_connection");
    return {
      check: {
        revision: 0,
        checked_at: 1,
        outcome: {
          status: "host_trust_required",
          fingerprint: "SHA256:fixture",
        },
        server_id: null,
        default_directory: null,
      },
      harnesses: [],
      trust_prompt: "exact native prompt",
      ticket: "check-ticket",
    };
  };
  await mount({ initial: draft, onSaved() {}, onCancel() {} });
  await click("Test connection");
  assert.deepEqual(calls[0].args.input.approved_host_prompts, []);
  assert.match(container.textContent, /SHA256:fixture/);
  await click("Trust this server and continue");
  assert.deepEqual(calls[1].args.input.approved_host_prompts, [
    "exact native prompt",
  ]);
});

test("editing saved credentials preserves them unless explicitly changed", async () => {
  const draft = { ...connection(), revision: 3 };
  let saved;
  handler = (command, args) => {
    if (command === "execution_connection_has_credentials") return true;
    assert.equal(command, "save_execution_connection");
    assert.equal(
      args.credentials,
      null,
      "unchanged credential fields must not erase a saved secret",
    );
    return { ...args.connection, revision: 4 };
  };
  await mount({
    initial: draft,
    onSaved(value) {
      saved = value;
    },
    onCancel() {},
  });
  assert.equal(
    container.querySelector('[role="checkbox"]').getAttribute("aria-checked"),
    "true",
  );
  await click("Save connection");
  assert.equal(saved.revision, 4);
  assert.deepEqual(
    calls.map((call) => call.command),
    ["execution_connection_has_credentials", "save_execution_connection"],
  );
});
