import assert from "node:assert/strict";
import test from "node:test";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ConnectionsOnboardingStep } from "./ConnectionsOnboardingStep.tsx";

test("onboarding can continue with no connections and never discovers or starts a local harness", async () => {
  const calls = [];
  const bridge = {
    invoke(command) {
      calls.push(command);
      if (command === "list_execution_connections") return Promise.resolve([]);
      if (command === "get_identity")
        return Promise.resolve({ pubkey: "owner" });
      return Promise.reject(new Error(`Unexpected IPC: ${command}`));
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  window.__TAURI_INTERNALS__ = bridge;
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  let continued = false;
  try {
    await act(async () => {
      root.render(
        React.createElement(
          QueryClientProvider,
          { client },
          React.createElement(ConnectionsOnboardingStep, {
            onContinue: () => {
              continued = true;
            },
          }),
        ),
      );
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    const button = [...container.querySelectorAll("button")].find(
      (item) => item.textContent === "Continue to Buzz",
    );
    assert.ok(button);
    assert.equal(button.disabled, false);
    await act(async () => button.click());
    assert.equal(continued, true);
    assert.ok(calls.includes("list_execution_connections"));
    assert.ok(
      calls.every((command) =>
        ["get_identity", "list_execution_connections"].includes(command),
      ),
    );
  } finally {
    await act(async () => root.unmount());
    client.clear();
    container.remove();
  }
});
