// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { createRawSnippet, flushSync, tick } from "svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import Button from "./Button.svelte";
import Dialog from "./Dialog.svelte";
import IconButton from "./IconButton.svelte";
import InlineBanner from "./InlineBanner.svelte";
import Menu from "./Menu.svelte";
import ProgressBar from "./ProgressBar.svelte";
import SegmentedControl from "./SegmentedControl.svelte";
import TextField from "./TextField.svelte";
import ToastRegion from "./ToastRegion.svelte";
import { dismissToast, showToast, TOAST_DURATION_MS, toasts } from "./toasts.svelte";

const html = (markup: string) => createRawSnippet(() => ({ render: () => markup }));

beforeAll(() => {
  // jsdom has no modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

afterEach(() => {
  cleanup();
  [...toasts].forEach((toast) => dismissToast(toast.id));
  vi.useRealTimers();
});

describe("Button", () => {
  it("calls onclick, but not while loading", async () => {
    const onclick = vi.fn();
    const { rerender } = render(Button, { onclick, children: html("<span>Save</span>") });

    await fireEvent.click(screen.getByRole("button"));
    expect(onclick).toHaveBeenCalledTimes(1);

    await rerender({ loading: true });
    await fireEvent.click(screen.getByRole("button"));
    expect(onclick).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button").getAttribute("aria-busy")).toBe("true");
  });
});

describe("IconButton", () => {
  it("uses the label as accessible name and tooltip, and reports toggled", () => {
    render(IconButton, { icon: "trash", label: "Delete paper", toggled: true });

    const button = screen.getByRole("button", { name: "Delete paper" });
    expect(button.title).toBe("Delete paper");
    expect(button.getAttribute("aria-pressed")).toBe("true");
  });
});

describe("TextField", () => {
  it("shows an error as icon and text, linked to the input", () => {
    render(TextField, { label: "No Ref", error: "Wrong number" });

    const input = screen.getByLabelText("No Ref");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    const message = document.getElementById(input.getAttribute("aria-describedby")!)!;
    expect(message.textContent).toContain("Wrong number");
    expect(message.querySelector('[data-icon="error"]')).not.toBeNull();
  });
});

describe("SegmentedControl", () => {
  it("reports the chosen option", async () => {
    const onchange = vi.fn();
    render(SegmentedControl, {
      label: "Appearance",
      value: "system",
      options: [
        { value: "system", label: "System" },
        { value: "dark", label: "Dark" },
      ],
      onchange,
    });

    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(onchange).toHaveBeenCalledWith("dark");
  });
});

describe("Menu", () => {
  const setup = async () => {
    const select = vi.fn();
    render(Menu, {
      label: "Export",
      items: [
        { label: "First", onselect: vi.fn() },
        { label: "Off", disabled: true, onselect: vi.fn() },
        { separator: true as const },
        { label: "Last", onselect: select },
      ],
    });
    const trigger = screen.getByRole("button", { name: "Export" });
    await fireEvent.click(trigger);
    await tick();
    return { trigger, select };
  };
  const press = (key: string) => fireEvent.keyDown(document.activeElement!, { key });

  it("moves with the arrow keys, wraps, and skips disabled items", async () => {
    await setup();
    expect(document.activeElement?.textContent).toContain("First");
    await press("ArrowDown");
    expect(document.activeElement?.textContent).toContain("Last");
    await press("ArrowDown");
    expect(document.activeElement?.textContent).toContain("First");
    await press("ArrowUp");
    expect(document.activeElement?.textContent).toContain("Last");
  });

  it("closes on Escape and returns focus to the trigger", async () => {
    const { trigger } = await setup();
    await press("Escape");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("runs the chosen item and closes", async () => {
    const { select } = await setup();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Last" }));
    expect(select).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes on a click outside", async () => {
    await setup();
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("Dialog", () => {
  const props = (onclose: () => void, busy = false) => ({
    open: true,
    busy,
    title: "Create library",
    onclose,
    children: html('<input aria-label="Library name" />'),
    actions: html("<button>Cancel</button>"),
  });

  it("focuses the first field and gives focus back when closed", async () => {
    const opener = document.body.appendChild(document.createElement("button"));
    opener.focus();
    const { rerender } = render(Dialog, props(vi.fn()));
    await tick();
    expect(document.activeElement).toBe(screen.getByLabelText("Library name"));

    await rerender({ open: false });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("asks to close on Escape", async () => {
    const onclose = vi.fn();
    render(Dialog, props(onclose));
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("cannot be closed and disables its controls while busy", async () => {
    const onclose = vi.fn();
    render(Dialog, props(onclose, true));
    await tick();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancel" }).matches(":disabled")).toBe(true);
  });
});

describe("Toast", () => {
  it("removes success toasts after 4 seconds but keeps errors until closed", async () => {
    vi.useFakeTimers();
    render(ToastRegion);
    showToast("3 PDFs added.", "success");
    showToast("Could not open name.pdf.", "error");
    flushSync();

    const error = screen.getByText("Could not open name.pdf.");
    expect(error.closest("[aria-live]")?.getAttribute("aria-live")).toBe("assertive");

    vi.advanceTimersByTime(TOAST_DURATION_MS);
    flushSync();
    expect(screen.queryByText("3 PDFs added.")).toBeNull();
    expect(screen.queryByText("Could not open name.pdf.")).not.toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByText("Could not open name.pdf.")).toBeNull();
  });
});

describe("InlineBanner", () => {
  it("announces errors as alerts, with an icon next to the text", () => {
    render(InlineBanner, { tone: "error", title: "Couldn't save." });
    const banner = screen.getByRole("alert");
    expect(banner.textContent).toContain("Couldn't save.");
    expect(banner.querySelector('[data-icon="error"]')).not.toBeNull();
  });
});

describe("ProgressBar", () => {
  it("clamps the value and omits it when indeterminate", async () => {
    const { rerender } = render(ProgressBar, { label: "Indexing", value: 90, max: 80 });
    const bar = screen.getByRole("progressbar", { name: "Indexing" });
    expect(bar.getAttribute("aria-valuenow")).toBe("80");

    await rerender({ indeterminate: true });
    expect(bar.hasAttribute("aria-valuenow")).toBe(false);
  });
});
