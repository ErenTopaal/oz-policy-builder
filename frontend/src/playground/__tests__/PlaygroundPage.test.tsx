// scaffold-level tests for the /playground route shell.
//
// these check the chrome only: tab placeholders render, tab switching
// works, the share-badge surfaces the snapshot id from the URL, and the
// landing route still renders the existing Hero (no visual regression).
// wave-2 sibling tests cover the actual panel contents.

import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { AppRoutes } from "../../App";

describe("PlaygroundPage", () => {
  it("renders all four tab placeholders (spec tab initially active)", () => {
    render(
      <MemoryRouter initialEntries={["/playground"]}>
        <AppRoutes />
      </MemoryRouter>
    );
    // tab buttons present
    expect(screen.getByRole("tab", { name: "Spec" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Source" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Simulate" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Bundle" })).toBeTruthy();
    // spec is the default active tab
    expect(screen.getByText("Spec — coming in wave 2")).toBeTruthy();
  });

  it("switches active tab on click", () => {
    render(
      <MemoryRouter initialEntries={["/playground"]}>
        <AppRoutes />
      </MemoryRouter>
    );
    fireEvent.click(screen.getByRole("tab", { name: "Source" }));
    // SourceTab is implemented — without artifacts it shows the empty marker.
    expect(screen.getByText("no source yet — synthesize first")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "Simulate" }));
    // SimulateTab is implemented — without a report it shows the empty marker.
    expect(screen.getByText("no simulation yet — synthesize first")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "Bundle" }));
    expect(screen.getByText("Bundle — coming in wave 2")).toBeTruthy();
  });

  it("renders the snapshot id from the URL into the share badge", () => {
    render(
      <MemoryRouter initialEntries={["/playground/s/abc12345"]}>
        <AppRoutes />
      </MemoryRouter>
    );
    const badge = screen.getByTestId("share-badge");
    expect(badge.textContent).toContain("share: abc12345");
  });

  it("landing route renders the existing Hero section", () => {
    const { container } = render(
      <MemoryRouter initialEntries={["/"]}>
        <AppRoutes />
      </MemoryRouter>
    );
    // Hero renders a <section id="top">; presence proves landing is intact.
    const hero = container.querySelector("section#top");
    expect(hero).not.toBeNull();
  });
});
