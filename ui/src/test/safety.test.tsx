import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../App";
import { setBackend } from "../lib/backend";
import { createMockBackend } from "../lib/mockBackend";
import { tauriBackend } from "../lib/tauriBackend";
import { useStore } from "../state/store";

const DESTRUCTIVE = /delete|remove|clean|recycle|erase|wipe|trash|purge|move to/i;

describe("safety: nothing in milestone (b) can delete", () => {
  it("the backend API has no delete/move/write operation", () => {
    for (const name of Object.keys(tauriBackend)) expect(name).not.toMatch(DESTRUCTIVE);
  });

  beforeEach(() => {
    setBackend(createMockBackend());
    useStore.setState({ onboardingDone: true, scans: {}, page: "overview", drives: null });
  });

  it("the Explore right-click menu only offers safe actions", async () => {
    render(<App />);
    const scanButtons = await screen.findAllByRole("button", { name: "Scan" });
    fireEvent.click(scanButtons[0] as HTMLElement);
    await screen.findByRole("button", { name: "Scan again" }, { timeout: 6000 });
    act(() => useStore.getState().go("explore"));

    const grid = await screen.findByRole("grid", {}, { timeout: 4000 });
    await waitFor(() => expect(grid).toHaveAttribute("aria-activedescendant"));
    // Give the first page of rows time to load, then open the menu from the keyboard.
    await waitFor(async () => {
      fireEvent.keyDown(grid, { key: "ContextMenu" });
      expect(await screen.findByRole("menu")).toBeInTheDocument();
    });
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem");
    const labels = items.map((i) => i.textContent ?? "");
    expect(labels).toContain("Open in File Explorer");
    expect(labels).toContain("Copy path");
    for (const l of labels) expect(l).not.toMatch(DESTRUCTIVE);
  }, 15_000);
});
