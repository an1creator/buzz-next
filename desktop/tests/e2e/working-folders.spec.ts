import { expect, test } from "@playwright/test";
import { installMockBridge } from "../helpers/bridge";

test("channel working folders preserve inheritance and Save/Cancel boundaries", async ({
  page,
}, testInfo) => {
  await installMockBridge(page);
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.evaluate(() => {
    const host = window as unknown as {
      __TAURI_INTERNALS__: {
        invoke: (
          command: string,
          args?: Record<string, unknown>,
        ) => Promise<unknown>;
      };
      folderWrites: Record<string, unknown>[];
    };
    const original = host.__TAURI_INTERNALS__.invoke;
    host.folderWrites = [];
    let saved: string | null = null;
    let revision = 0;
    host.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === "working_folder_targets")
        return [
          {
            id: "server",
            agentPubkey: "b".repeat(64),
            label: "MyServer · shared-codex",
          },
        ];
      if (command === "get_working_folder_settings") {
        await new Promise((resolve) => setTimeout(resolve, 75));
        return {
          targetId: "server",
          supported: true,
          defaultEnvKey: "BUZZ_ACP_WORKSPACE",
          remote: true,
          folder: { path: saved, revision },
        };
      }
      if (command === "set_channel_working_folder") {
        host.folderWrites.push(args ?? {});
        saved = (args?.path as string | null) ?? null;
        return { path: saved, revision: ++revision };
      }
      return original(command, args);
    };
  });
  await page.getByTestId("channel-management-trigger").click();
  await page.getByTestId("channel-working-folders").click();
  const dialog = page.getByRole("dialog", {
    name: "Working folders",
    exact: true,
  });
  const input = dialog.getByRole("textbox", {
    name: "Working folder",
    exact: true,
  });
  await expect(input).toHaveValue("");
  await expect(input).toHaveAttribute(
    "placeholder",
    "Use each agent’s default folder",
  );
  await input.fill("/home/n1creator/projects/hw");
  await expect(dialog.locator("#working-folder-computer")).toBeDisabled();
  await page.screenshot({
    path: testInfo.outputPath("working-folders.png"),
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { folderWrites: unknown[] }).folderWrites.length,
    ),
  ).toBe(0);
  await page.getByTestId("channel-working-folders").click();
  await expect(input).toHaveValue("");
  await input.fill("/home/n1creator/projects/hw");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.getByTestId("channel-working-folders").click();
  await expect(input).toHaveValue("/home/n1creator/projects/hw");
  await input.fill("");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  const writes = await page.evaluate(
    () =>
      (window as unknown as { folderWrites: Record<string, unknown>[] })
        .folderWrites,
  );
  expect(writes).toHaveLength(2);
  expect(writes[0]).toMatchObject({
    targetId: "server",
    path: "/home/n1creator/projects/hw",
    revision: 0,
  });
  expect(writes[1]).toMatchObject({
    targetId: "server",
    path: null,
    revision: 1,
  });
});
