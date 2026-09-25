import assert from "node:assert/strict";
import test from "node:test";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ExecutionFields } from "./ExecutionFields.tsx";

test("cancelling a folder check cancels native work and ignores its late result", async () => {
  const calls = [];
  let finishDirectory;
  const directory = new Promise((resolve) => {
    finishDirectory = resolve;
  });
  const connection = {
    id: crypto.randomUUID(),
    name: "Server",
    revision: 1,
    target: {
      type: "ssh",
      endpoint: { mode: "config", path: "/config", alias: "server" },
    },
    defaults: { harness_id: null, model: null },
    check: null,
  };
  const bridge = {
    invoke(command, args) {
      calls.push({ command, args });
      if (command === "get_identity")
        return Promise.resolve({ pubkey: "owner" });
      if (command === "list_execution_connections")
        return Promise.resolve([connection]);
      if (command === "test_execution_connection")
        return Promise.resolve({
          check: { revision: 1, checked_at: 1, outcome: { status: "ready" } },
          harnesses: [],
          default_directory: "/automatic",
          ticket: "fixture",
        });
      if (command === "validate_execution_directory") return directory;
      if (command === "cancel_execution_connection_check")
        return Promise.resolve();
      return Promise.reject(new Error(`Unexpected IPC: ${command}`));
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  window.__TAURI_INTERNALS__ = bridge;
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(["identity"], { pubkey: "owner" });
  client.setQueryData(["execution-connections", "owner"], [connection]);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  async function click(label) {
    const button = [...container.querySelectorAll("button")].find(
      (item) => item.textContent === label,
    );
    assert.ok(button, label);
    await act(async () => button.click());
  }
  try {
    await act(async () =>
      root.render(
        React.createElement(
          QueryClientProvider,
          { client },
          React.createElement(ExecutionFields, {
            value: {
              connection_id: connection.id,
              harness_id: null,
              model: null,
              directory: { mode: "automatic" },
            },
            onChange() {},
          }),
        ),
      ),
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    await click("Check folder");
    const check = calls.find(
      (call) => call.command === "validate_execution_directory",
    );
    assert.ok(check.args.operationId);
    await click("Cancel folder check");
    assert.ok(
      calls.some(
        (call) =>
          call.command === "cancel_execution_connection_check" &&
          call.args.operationId === check.args.operationId,
      ),
    );
    await act(async () => {
      finishDirectory({ path: "/stale-folder" });
    });
    assert.ok(!container.textContent.includes("/stale-folder"));
  } finally {
    await act(async () => root.unmount());
    client.clear();
    container.remove();
  }
});
