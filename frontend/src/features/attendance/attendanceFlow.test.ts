import { describe, expect, it, vi } from "vitest";
import { getOrCreateIdempotencyKey, submitAttendanceAfterProfile } from "./attendanceFlow";

describe("QR attendance flow", () => {
  it("profile_load_failure_never_posts_attendance", async () => {
    const create = vi.fn();
    await expect(submitAttendanceAfterProfile("qr", "key", async () => { throw new Error("profile failed"); }, create)).rejects.toThrow("profile failed");
    expect(create).not.toHaveBeenCalled();
  });

  it("empty_profile_never_posts_attendance", async () => {
    const create = vi.fn();
    await expect(submitAttendanceAfterProfile("qr", "key", async () => ({ id: "", display_name: "", email: null, employee_code: null }), create)).rejects.toThrow("打刻は記録されていません");
    expect(create).not.toHaveBeenCalled();
  });

  it("scan_reuses_idempotency_key_after_retry", () => {
    const values = new Map<string, string>();
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
    const one = getOrCreateIdempotencyKey("same-token", storage);
    const two = getOrCreateIdempotencyKey("same-token", storage);
    expect(one).toMatch(/^[0-9a-f-]{36}$/i);
    expect(two).toBe(one);
  });
});
