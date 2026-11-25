import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPageSaver, SAVE_DELAY_MS } from "./pageSaver";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("createPageSaver", () => {
  it("saves only the page where the scrolling stopped", () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const saver = createPageSaver(save);
    for (const page of [2, 3, 4]) {
      saver.change(page);
      vi.advanceTimersByTime(SAVE_DELAY_MS - 1);
    }
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(save).toHaveBeenCalledExactlyOnceWith(4);
  });

  it("saves a waiting page at once on flush, and only once", () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const saver = createPageSaver(save);
    saver.flush();
    saver.change(5);
    saver.flush();
    saver.flush();
    vi.runAllTimers();
    expect(save).toHaveBeenCalledExactlyOnceWith(5);
  });

  it("keeps working after a failed save", async () => {
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    const save = vi.fn().mockRejectedValueOnce(new Error("disk full")).mockResolvedValue(undefined);
    const saver = createPageSaver(save);
    saver.change(2);
    await vi.runAllTimersAsync();
    saver.change(3);
    await vi.runAllTimersAsync();
    expect(save).toHaveBeenLastCalledWith(3);
    expect(log).toHaveBeenCalledTimes(1);
  });
});
