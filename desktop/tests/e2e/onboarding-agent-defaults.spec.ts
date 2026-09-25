import { expect, test, type Page } from "@playwright/test";
import { installMockBridge } from "../helpers/bridge";
import { passThroughBackupStep } from "../helpers/onboarding";

async function openConnections(page: Page) {
  await installMockBridge(
    page,
    {},
    { skipCommunitySeed: true, skipOnboardingSeed: true },
  );
  await page.goto("/");
  await page.getByRole("button", { name: "Create a new identity key" }).click();
  await page.getByRole("button", { name: "Create my private key" }).click();
  await passThroughBackupStep(page);
  await expect(page.getByTestId("onboarding-connections")).toBeVisible();
}

test("onboarding permits conversations without an implicit local connection", async ({
  page,
}) => {
  await openConnections(page);
  await expect(
    page.getByText("No connections yet", { exact: true }),
  ).toBeVisible();
  const proceed = page.getByRole("button", {
    name: "Continue to Buzz",
    exact: true,
  });
  await expect(proceed).toBeEnabled();
  await proceed.click();
  await expect(page.getByTestId("onboarding-connections")).not.toBeVisible();
});

test("onboarding uses explicit device choice and Cancel leaves Connections empty", async ({
  page,
}) => {
  await openConnections(page);
  await page
    .getByRole("button", { name: "Add connection", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "This device", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "SSH server", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "This device", exact: true }).click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page.getByText("No connections yet", { exact: true }),
  ).toBeVisible();
});

test("onboarding saves an unchecked SSH connection without requiring local harnesses", async ({
  page,
}) => {
  await openConnections(page);
  await page
    .getByRole("button", { name: "Add connection", exact: true })
    .click();
  await page.getByRole("button", { name: "SSH server", exact: true }).click();
  await page.getByLabel("Name", { exact: true }).fill("Remote work");
  await page.getByLabel("Host", { exact: true }).fill("server.example");
  await page.getByLabel("Username", { exact: true }).fill("developer");
  await page
    .getByRole("button", { name: "Save connection", exact: true })
    .click();
  await expect(page.getByTestId("connection-detail")).toBeVisible();
  await expect(page.getByText("Remote work", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Continue to Buzz", exact: true }),
  ).toBeEnabled();
});
