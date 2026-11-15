import { useLocation } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";
import { safeReturnTo } from "../auth/returnTo";

export function LoginPage() {
  const { status, error, demoUsers, mode, login, selectDemoUser } = useAuth();
  const location = useLocation();
  const returnTo = safeReturnTo((location.state as { returnTo?: string } | null)?.returnTo ?? "/attendance");
  return <main className="login-screen">
    <section className="login-card">
      <div className="brand login-brand"><span className="brand-mark">S</span><span>shiftline</span></div>
      <p className="eyebrow">QR ATTENDANCE & ACCESS</p>
      <h1>社員アカウントでログイン</h1>
      <p className="muted">認証済みのプロフィールを確認してから出退勤を記録します。</p>
      {error && <div className="alert alert-error" role="alert">{error}</div>}
      {mode === "dev" ? <div className="demo-users">
        <p className="field-label">ローカル開発用の架空ユーザー</p>
        {demoUsers.map((user) => <button className="demo-user" key={user.id} onClick={() => void selectDemoUser(user.id)} disabled={status === "loading"}>
          <span className="user-avatar">{user.display_name.slice(0, 1)}</span><span className="demo-user-copy"><strong>{user.display_name}</strong><small>{user.email ?? user.employee_code}</small></span><span aria-hidden="true">→</span>
        </button>)}
        {demoUsers.length === 0 && status !== "loading" && <p className="muted">APIから開発用ユーザーを取得できません。</p>}
      </div> : <button className="button button-primary button-wide" onClick={() => void login(returnTo)} disabled={status === "loading"}>Microsoft Entra ID でログイン</button>}
      <div className="login-footnote"><span className="dot dot-green" />HRの所属・職務コードに基づく権限を利用</div>
    </section>
    <p className="login-caption">SHIFTLINE <span>—</span> PORTFOLIO SYSTEM</p>
  </main>;
}

export function ForbiddenPage() {
  return <main className="login-screen"><section className="login-card"><p className="eyebrow">403 / ACCESS CONTROL</p><h1>この画面への権限がありません</h1><p className="muted">権限はバックエンドで確認されます。管理画面は管理者だけが利用できます。</p><a className="button button-primary" href="/attendance">出退勤画面へ戻る</a></section></main>;
}

export function NotFoundPage() {
  return <main className="login-screen"><section className="login-card"><p className="eyebrow">404 / NOT FOUND</p><h1>ページが見つかりません</h1><p className="muted">打刻QRまたはメニューから開き直してください。</p><a className="button button-primary" href="/attendance">出退勤画面へ戻る</a></section></main>;
}
