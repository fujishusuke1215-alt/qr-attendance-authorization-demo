import { NavLink, Outlet } from "react-router-dom";

export function AdminLayout() {
  return <div className="admin-shell">
    <aside className="admin-rail" aria-label="管理メニュー">
      <span className="rail-eyebrow">ADMIN CONSOLE</span>
      <NavLink to="/admin/qr">QRコード</NavLink>
      <NavLink to="/admin/permissions">権限管理</NavLink>
      <NavLink to="/admin/hr-sync">HR同期</NavLink>
    </aside>
    <div className="admin-content"><Outlet /></div>
  </div>;
}
