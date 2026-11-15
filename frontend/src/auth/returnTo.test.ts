import { describe, expect, it } from "vitest";
import { safeReturnTo } from "./returnTo";

describe("login return target", () => {
  it("login_return_target_rejects_external_urls", () => {
    expect(safeReturnTo("https://outside.example/path", "https://shiftline.example")).toBe("/attendance");
    expect(safeReturnTo("//outside.example/path", "https://shiftline.example")).toBe("/attendance");
    expect(safeReturnTo("/\\outside.example", "https://shiftline.example")).toBe("/attendance");
  });

  it("preserves_only_known_app_routes_and_valid_qr_scan_paths", () => {
    expect(safeReturnTo("/admin/permissions?tab=manual", "https://shiftline.example")).toBe("/admin/permissions?tab=manual");
    expect(safeReturnTo("/scan/qra1.check_in.3.signature", "https://shiftline.example")).toBe("/scan/qra1.check_in.3.signature");
    expect(safeReturnTo("/account/settings", "https://shiftline.example")).toBe("/attendance");
  });
});
