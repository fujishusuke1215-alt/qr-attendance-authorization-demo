import { useState } from "react";
import type { QrChannel } from "../../api/types";

export function QrRotationControl({ channel, onRotate, disabled = false }: { channel: QrChannel; onRotate: () => Promise<void>; disabled?: boolean }) {
  const [confirming, setConfirming] = useState(false);
  const [rotating, setRotating] = useState(false);

  async function confirmRotation() {
    setRotating(true);
    try {
      await onRotate();
      setConfirming(false);
    } finally {
      setRotating(false);
    }
  }

  return <>
    <button className="button button-outline" type="button" onClick={() => setConfirming(true)} disabled={disabled || rotating}>
      {rotating ? "更新中…" : "QRコードを更新"}
    </button>
    {confirming && <div className="dialog-backdrop" role="presentation">
      <section className="confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby={`rotate-title-${channel.kind}`} aria-describedby={`rotate-description-${channel.kind}`}>
        <div className="dialog-icon">↻</div>
        <p className="eyebrow">REVISION {channel.revision} → {channel.revision + 1}</p>
        <h2 id={`rotate-title-${channel.kind}`}>掲示中のQRを更新しますか？</h2>
        <p id={`rotate-description-${channel.kind}`}>更新すると現在のQRはすぐに無効になります。新しいQRを印刷して掲示し直してください。</p>
        <div className="dialog-actions"><button className="button button-quiet" type="button" onClick={() => setConfirming(false)} disabled={rotating}>キャンセル</button><button className="button button-danger" type="button" onClick={() => void confirmRotation()} disabled={rotating}>{rotating ? "更新中…" : "更新して旧QRを無効化"}</button></div>
      </section>
    </div>}
  </>;
}
