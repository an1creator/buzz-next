import { expect, test } from "@playwright/test";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

test("Connections start empty and cancelling SSH does not save", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "agents");
  await expect(
    page.getByText("No connections yet", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Add connection", exact: true })
    .click();
  await page.getByRole("button", { name: "SSH server", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("My development server");
  await page.getByLabel("Host", { exact: true }).fill("server.test");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page.getByText("No connections yet", { exact: true }),
  ).toBeVisible();
});

test("SSH trust is explicit and unchecked save remains separate", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "agents");
  await page
    .getByRole("button", { name: "Add connection", exact: true })
    .click();
  await page.getByRole("button", { name: "SSH server", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Development server");
  await page.getByLabel("Host", { exact: true }).fill("server.test");
  await page.getByLabel("Username", { exact: true }).fill("engineer");
  await page
    .getByRole("button", { name: "Test connection", exact: true })
    .click();
  await expect(
    page.getByText("SHA256:fixture-only", { exact: true }),
  ).toBeVisible();
  const trust = page.getByRole("button", {
    name: "Trust this server and continue",
    exact: true,
  });
  await trust.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText("Codex — Server", { exact: true })).toBeVisible();
  await page.screenshot({
    path: "test-results/connections-ssh-tested.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Save connection", exact: true })
    .click();
  await expect(page.getByTestId("connection-detail")).toBeVisible();
  await expect(
    page.getByText("Development server", { exact: true }),
  ).toBeVisible();
});

test("narrow form keeps the SSH fields and Save reachable by keyboard", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 700 });
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "agents");
  await page
    .getByRole("button", { name: "Add connection", exact: true })
    .click();
  await page.getByRole("button", { name: "SSH server", exact: true }).click();
  await page
    .getByLabel("Name", { exact: true })
    .fill("A long development server name for remote project work");
  await page
    .getByLabel("Host", { exact: true })
    .fill("development.server.example.test");
  await page.getByLabel("Username", { exact: true }).fill("engineer");
  const save = page.getByRole("button", {
    name: "Save connection",
    exact: true,
  });
  await save.focus();
  await expect(save).toBeFocused();
  await page.screenshot({
    path: "test-results/connections-narrow.png",
    fullPage: true,
  });
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("connection-detail")).toBeVisible();
});
