import { NavLink, Outlet } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";
import { paths } from "../app/paths";

export function AppLayout() {
  const { session, logout } = useAuth();
  const admin = session?.capabilities.admin ?? false;
  return <div className="app-shell">
    <header className="topbar">
      <NavLink to={paths.attendance} className="brand" aria-label="Shiftline ホーム"><span className="brand-mark">S</span><span>shiftline<span className="brand-suffix"> / DEMO</span></span></NavLink>
      <nav className="main-nav" aria-label="メインナビゲーション">
        <NavLink to={paths.attendance}>出退勤</NavLink>
        {admin && <NavLink to={paths.qrAdmin}>QR管理</NavLink>}
        {admin && <NavLink to={paths.permissions}>権限</NavLink>}
        {admin && <NavLink to={paths.hrSync}>HR同期</NavLink>}
      </nav>
      <div className="user-tools"><span className="user-chip"><span className="user-avatar">{session?.user.display_name.slice(0, 1)}</span><span className="user-name">{session?.user.display_name}</span></span><button className="button button-quiet button-small" onClick={() => void logout()}>ログアウト</button></div>
    </header>
    <main className="main-content"><Outlet /></main>
    <footer className="site-footer">Independent portfolio demo <span>・</span> fictional HR data</footer>
  </div>;
}
