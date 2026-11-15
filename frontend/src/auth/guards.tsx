import { Navigate, Outlet, useLocation } from "react-router-dom";
import { useAuth } from "./AuthContext";

export function RequireAuth() {
  const auth = useAuth();
  const location = useLocation();
  if (auth.status === "loading") return <div className="full-state"><span className="spinner" />認証状態を確認しています</div>;
  if (auth.status === "error") return <div className="full-state"><p role="alert">{auth.error}</p><button className="button" onClick={() => void auth.login(`${location.pathname}${location.search}`)}>再ログイン</button></div>;
  if (auth.status !== "ready") return <Navigate to="/login" replace state={{ returnTo: `${location.pathname}${location.search}` }} />;
  return <Outlet />;
}

export function RequireAdmin() {
  const auth = useAuth();
  if (!auth.session?.capabilities.admin) return <Navigate to="/forbidden" replace />;
  return <Outlet />;
}
