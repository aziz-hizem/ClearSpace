import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../App";
import { setBackend } from "../lib/backend";
import { createMockBackend } from "../lib/mockBackend";
import { useStore } from "../state/store";

describe("app shell", () => {
  beforeEach(() => {
    setBackend(createMockBackend());
    useStore.setState({ onboardingDone: true, page: "overview", drives: null, scans: {} });
  });

  it("navigates with the sidebar and Ctrl+number, marking the current page", async () => {
    render(<App />);
    const nav = screen.getByRole("navigation", { name: "Main" });
    expect(nav).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Settings/ }));
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "1", ctrlKey: true });
    expect(screen.getByRole("heading", { level: 1, name: "Overview" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Overview/ })).toHaveAttribute("aria-current", "page");
  });

  it("lists drives with an accessible used-space meter", async () => {
    render(<App />);
    const meters = await screen.findAllByRole("meter");
    expect(meters[0]).toHaveAccessibleName(/Space used on Local Disk \(C:\)/);
    expect(screen.getByText("No disk inserted, or the drive isn't responding.")).toBeInTheDocument();
  });

  it("switches theme from Settings and persists it", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Settings/ }));
    fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("cs.theme")).toBe('"dark"');
  });
});
