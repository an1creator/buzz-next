import { expect, test } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const BAKED_DEFAULTS = [
  { key: "BUZZ_AGENT_PROVIDER", value: "anthropic", masked: false },
  {
    key: "BUZZ_AGENT_MODEL",
    value: "claude-opus-4-8",
    masked: false,
  },
  { key: "BUZZ_AGENT_THINKING_EFFORT", value: "high", masked: false },
  { key: "ANTHROPIC_API_KEY", value: "sk-ant-baked-test", masked: true },
];

// UI persistence checks use mock IPC; native validation is covered by Rust tests.
const AGENT_PUBKEY = TEST_IDENTITIES.tyler.pubkey;
const AGENT_NAME = "Tyler Agent";
const PERSONA_ID = "persona-edit-e2e";

/**
 * Open the Edit Agent dialog for the seeded managed agent via the profile
 * panel (agents view → agent card → Edit quick action) — EditAgentDialog's
 * only mount path.
 */
async function openEditDialog(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByTestId("open-agents-view").click();

  const agentButton = page.getByRole("button", {
    name: `${AGENT_NAME} agent profile`,
  });
  await expect(agentButton).toBeVisible({ timeout: 10_000 });
  await agentButton.click();

  await expect(page.getByTestId("user-profile-panel")).toBeVisible({
    timeout: 10_000,
  });
  await page.getByTestId("user-profile-edit-agent").click();

  await expect(page.getByTestId("edit-agent-dialog")).toBeVisible({
    timeout: 10_000,
  });
  await expect(
    page.getByRole("combobox", { name: "Connection", exact: true }),
  ).toBeVisible();
}

test.describe("agent definition dialog", () => {
  test("owner-only-access build shows disabled agent access with an explanation", async ({
    page,
  }) => {
    await installMockBridge(page, {
      ownerOnlyAccessBuild: true,
      bakedBuildEnv: BAKED_DEFAULTS,
    });
    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    await page.getByTestId("new-agent-card").click();

    const dialog = page.getByRole("dialog");
    await dialog.getByRole("button", { name: "Advanced", exact: true }).click();

    await expect(dialog.getByTestId("agent-respond-to")).toBeVisible();
    await expect(dialog.locator("#agent-respond-to")).toBeDisabled();
    await expect(dialog.locator("#agent-respond-to")).toContainText(
      "Only me (default)",
    );
    await expect(
      dialog.getByTestId("agent-respond-to-disabled-reason"),
    ).toHaveText("This build disallows changing this setting.");
  });
});

test.describe("edit agent dialog", () => {
  test("owner-only-access build shows a disabled owner-only access control with an explanation", async ({
    page,
  }) => {
    await installMockBridge(page, {
      ownerOnlyAccessBuild: true,
      bakedBuildEnv: BAKED_DEFAULTS,
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
          respondTo: "anyone",
        },
      ],
    });

    await openEditDialog(page);

    const accessControl = page.getByTestId("agent-respond-to");
    await expect(accessControl).toBeVisible();
    await expect(page.locator("#agent-respond-to")).toBeDisabled();
    await expect(page.locator("#agent-respond-to")).toContainText(
      "Only me (default)",
    );
    await expect(
      page.getByTestId("agent-respond-to-disabled-reason"),
    ).toHaveText("This build disallows changing this setting.");
  });

  test("OSS build keeps the managed-agent access control", async ({ page }) => {
    await installMockBridge(page, {
      bakedBuildEnv: BAKED_DEFAULTS,
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
    });

    await openEditDialog(page);

    await expect(page.getByTestId("agent-respond-to")).toBeVisible();
  });

  test("edits the agent name and persists it across a dialog reopen", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
    });

    await openEditDialog(page);

    const nameInput = page.locator("#edit-agent-name");
    await expect(nameInput).toHaveValue(AGENT_NAME);
    await nameInput.fill("Tyler Agent Renamed");

    await page.getByTestId("edit-agent-dialog-submit").click();
    await expect(page.getByTestId("edit-agent-dialog")).not.toBeVisible();

    // Reopen: the dialog re-reads the managed-agents store, proving the save
    // survived the dialog lifecycle rather than living in local state. (The
    // panel HEADER is not asserted — it renders the relay profile name, which
    // the update path does not touch.)
    await page.getByTestId("user-profile-edit-agent").click();
    await expect(page.locator("#edit-agent-name")).toHaveValue(
      "Tyler Agent Renamed",
      { timeout: 10_000 },
    );
  });

  test("saves connection-scoped execution and preserves existing environment", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          status: "stopped",
          channelNames: ["agents"],
          envVars: { PROJECT_FLAG: "retained" },
        },
      ],
    });
    await openEditDialog(page);
    await page
      .getByRole("button", { name: "Add connection", exact: true })
      .click();
    await page
      .getByRole("button", { name: "This device", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Save connection", exact: true })
      .click();
    await page.getByRole("combobox", { name: "Harness", exact: true }).click();
    await page
      .getByRole("option", { name: "Codex — Server", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Custom model", exact: true })
      .click();
    await page.getByLabel("Model ID", { exact: true }).fill("my-custom-model");
    await page.getByTestId("edit-agent-dialog-submit").click();
    await expect(page.getByTestId("edit-agent-dialog")).not.toBeVisible();
    await page.getByTestId("user-profile-edit-agent").click();
    await expect(
      page.getByRole("combobox", { name: "Harness", exact: true }),
    ).toHaveAttribute("data-value", "server-codex");
    await expect(
      page.getByRole("combobox", { name: "Model", exact: true }),
    ).toHaveAttribute("data-value", "my-custom-model");
    await page.getByText("Advanced", { exact: true }).click();
    await expect(page.locator('input[value="retained"]')).toBeVisible();
  });

  test("profile Edit opens the persona editor for a persona-linked agent", async ({
    page,
  }) => {
    // A persona-linked agent inherits ACP transport from its definition, so the
    // profile Edit action must open the persona editor.
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: AGENT_NAME,
          personaId: PERSONA_ID,
          status: "stopped",
          channelNames: ["agents"],
        },
      ],
      personas: [
        {
          id: PERSONA_ID,
          displayName: "Edit E2E Persona",
          systemPrompt: "You are the edit-agent e2e persona.",
        },
      ],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();

    // Persona-linked agents render grouped under the persona's card name.
    const agentButton = page.getByRole("button", {
      name: "Edit E2E Persona agent profile",
    });
    await expect(agentButton).toBeVisible({ timeout: 10_000 });
    await agentButton.click();

    await expect(page.getByTestId("user-profile-panel")).toBeVisible({
      timeout: 10_000,
    });
    await page.getByTestId("user-profile-edit-agent").click();

    // Persona editor opens with the definition-owned ACP command field.
    await expect(page.getByTestId("persona-dialog")).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByTestId("edit-agent-dialog")).not.toBeVisible();
    await page.getByRole("tab", { name: "Customize for this agent" }).click();
    await expect(page.locator("#persona-acp-command")).toBeVisible();
  });
});
