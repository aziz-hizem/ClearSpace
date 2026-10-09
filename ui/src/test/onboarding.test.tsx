import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../App";
import { setBackend } from "../lib/backend";
import { createMockBackend } from "../lib/mockBackend";
import { useStore } from "../state/store";

describe("first-run onboarding", () => {
  beforeEach(() => {
    setBackend(createMockBackend());
    useStore.setState({ onboardingDone: false, page: "overview" });
  });

  it("walks through three screens ending with the safety promise", () => {
    render(<App />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAccessibleName("See what fills your drive");
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(
      screen.getByRole("heading", { name: "Nothing is deleted without your approval" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem("cs.onboardingDone")).toBe("true");
  });

  it("can be skipped with Escape", () => {
    render(<App />);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
