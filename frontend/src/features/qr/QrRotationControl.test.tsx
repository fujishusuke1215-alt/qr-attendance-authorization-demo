import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { QrChannel } from "../../api/types";
import { QrRotationControl } from "./QrRotationControl";

const channel: QrChannel = { kind: "check_in", revision: 4, active: true, token: "token", scan_url: "https://demo.test/scan/token", issued_at: "2026-11-15T00:00:00Z", issued_by: null };

describe("QR revision controls", () => {
  it("qr_rotation_requires_confirmation", async () => {
    const user = userEvent.setup();
    const onRotate = vi.fn().mockResolvedValue(undefined);
    render(<QrRotationControl channel={channel} onRotate={onRotate} />);
    await user.click(screen.getByRole("button", { name: "QRコードを更新" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("現在のQRはすぐに無効になります");
    expect(onRotate).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "更新して旧QRを無効化" }));
    expect(onRotate).toHaveBeenCalledOnce();
  });
});
