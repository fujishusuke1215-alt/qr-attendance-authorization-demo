import { useEffect, useState } from "react";
import * as QRCode from "qrcode";
import { getQrState, rotateQr } from "../../api/adminApi";
import { ApiError } from "../../api/client";
import type { QrChannel, QrKind, QrState } from "../../api/types";
import { QrRotationControl } from "./QrRotationControl";

function dateTime(value: string) {
  return new Intl.DateTimeFormat("ja-JP", { timeZone: "Asia/Tokyo", dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export function QrAdminPage() {
  const [data, setData] = useState<QrState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busyKind, setBusyKind] = useState<QrKind | null>(null);

  async function load() {
    setError(null);
    try { setData(await getQrState()); }
    catch (cause) { setError(cause instanceof ApiError ? cause.message : "QR設定を取得できませんでした。"); }
  }

  useEffect(() => { void load(); }, []);

  async function rotate(channel: QrChannel) {
    setBusyKind(channel.kind);
    setNotice(null);
    try {
      await rotateQr(channel.kind, channel.revision);
      await load();
      setNotice("新しいQRを発行しました。掲示中の旧QRは無効です。新しいコードへ差し替えてください。");
    } catch (cause) {
      setError(cause instanceof ApiError && cause.status === 409 ? "別の管理者が先に更新しました。最新状態を読み込んでください。" : "QRの更新に失敗しました。");
      await load();
    } finally { setBusyKind(null); }
  }

  return <div className="page">
    <PageTitle eyebrow="QR MANAGEMENT" title="打刻QRコード" description="掲示する出勤・退勤QRを管理します。コードは毎日更新する必要がなく、管理者が明示的に更新したときだけ旧コードが無効になります。" />
    {error && <div className="alert alert-error" role="alert">{error}</div>}
    {notice && <div className="alert alert-success" role="status">{notice}</div>}
    {!data && !error && <div className="surface loading-card"><span className="spinner" />QRの状態を取得しています</div>}
    {data && <div className="qr-grid">{data.channels.map((channel) => <QrChannelCard key={channel.kind} channel={channel} busy={busyKind === channel.kind} onRotate={() => rotate(channel)} />)}</div>}
    {data && <section className="surface history-panel"><div className="section-heading"><div><p className="eyebrow">REVISION LOG</p><h2>更新履歴</h2></div><span className="count-chip">{data.history.length} 件</span></div>
      <div className="table-wrap"><table><thead><tr><th>用途</th><th>リビジョン</th><th>発行日時</th><th>発行者</th></tr></thead><tbody>{data.history.map((item) => <tr key={`${item.kind}-${item.revision}`}><td>{item.kind === "check_in" ? "出勤" : "退勤"}</td><td><span className="mono">r{item.revision}</span></td><td>{dateTime(item.issued_at)}</td><td>{item.issued_by ? item.issued_by.slice(0, 8) : "初期発行"}</td></tr>)}</tbody></table></div>
    </section>}
  </div>;
}

function QrChannelCard({ channel, busy, onRotate }: { channel: QrChannel; busy: boolean; onRotate: () => Promise<void> }) {
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    if (!channel.scan_url) { setQrDataUrl(null); return; }
    void QRCode.toDataURL(channel.scan_url, { width: 240, margin: 2, errorCorrectionLevel: "H" }).then((url) => { if (active) setQrDataUrl(url); }).catch(() => { if (active) setQrDataUrl(null); });
    return () => { active = false; };
  }, [channel.scan_url]);

  const title = channel.kind === "check_in" ? "出勤用" : "退勤用";
  return <section className="surface qr-card" aria-label={title}>
    <div className="qr-card-heading"><div><p className="eyebrow">{channel.kind === "check_in" ? "CHECK IN" : "CHECK OUT"}</p><h2>{title} QR</h2></div><span className="active-pill"><span className="dot dot-green" />有効</span></div>
    <div className="qr-card-body"><div className="qr-image-frame">{qrDataUrl ? <img src={qrDataUrl} alt={`${title}QRコード`} /> : <span className="spinner" />}</div><div className="qr-detail"><span className="detail-label">現在のリビジョン</span><strong className="revision-number">r{channel.revision.toString().padStart(2, "0")}</strong><span className="detail-label">最終発行</span><span className="detail-value">{dateTime(channel.issued_at)}</span><span className="detail-label">QRの有効期間</span><span className="detail-value">管理者が更新するまで</span></div></div>
    <div className="qr-card-actions">{channel.scan_url && qrDataUrl && <a className="button button-primary" href={qrDataUrl} download={`shiftline-${channel.kind}-r${channel.revision}.png`}>PNGをダウンロード</a>}<QrRotationControl channel={channel} onRotate={onRotate} disabled={busy} /></div>
    <p className="qr-card-footnote">更新後は revision {channel.revision} のQRが無効になります。</p>
  </section>;
}

function PageTitle({ eyebrow, title, description }: { eyebrow: string; title: string; description: string }) {
  return <header className="page-title"><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p>{description}</p></header>;
}
