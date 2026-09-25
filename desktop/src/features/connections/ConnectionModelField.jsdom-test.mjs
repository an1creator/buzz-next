import assert from "node:assert/strict";
import test from "node:test";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { ConnectionModelField } from "./ConnectionModelField.tsx";

test("model discovery is connection-scoped, disambiguates labels and ignores cancelled results", async () => {
  const calls = [];
  let finish;
  let result = Promise.resolve({
    models: [
      { id: "one", name: "Same" },
      { id: "two", name: "Same" },
    ],
  });
  const bridge = {
    invoke(command, args) {
      calls.push({ command, args });
      return command === "get_connection_models" ? result : Promise.resolve();
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  window.__TAURI_INTERNALS__ = bridge;
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const connection = {
    id: "server",
    revision: 2,
    name: "Server",
    target: { type: "local" },
    defaults: { harness_id: null, model: null },
    check: null,
  };
  async function click(label) {
    const button = [...container.querySelectorAll("button")].find(
      (b) => b.textContent === label,
    );
    assert.ok(button, label);
    await act(async () => button.click());
  }
  try {
    await act(async () =>
      root.render(
        React.createElement(ConnectionModelField, {
          connection,
          harnessId: "remote",
          value: null,
          onChange() {},
        }),
      ),
    );
    await click("Load models");
    assert.equal(calls[0].args.connectionId, "server");
    assert.equal(
      container.querySelector('[role="combobox"]').getAttribute("aria-expanded"),
      "true",
    );
    assert.match(container.textContent, /2 models loaded/);
    assert.ok(document.body.textContent.includes("Same · one"));
    assert.ok(document.body.textContent.includes("Same · two"));
    await act(async () => container.querySelector('[role="combobox"]').click());
    result = new Promise((resolve) => {
      finish = resolve;
    });
    await click("Load models");
    const request = calls.at(-1);
    await click("Cancel model check");
    assert.equal(calls.at(-1).command, "cancel_execution_connection_check");
    assert.equal(calls.at(-1).args.operationId, request.args.operationId);
    await act(async () =>
      finish({ models: [{ id: "stale", name: "Stale model" }] }),
    );
    assert.ok(!container.textContent.includes("Stale model"));
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});
