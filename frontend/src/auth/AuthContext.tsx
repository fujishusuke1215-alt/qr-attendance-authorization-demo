import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { apiRequest } from "../api/client";
import { configureApiCredentials } from "../api/client";
import type { CurrentSession, DemoUser } from "../api/types";
import { createAuthAdapter, type AuthAdapter } from "./adapter";
import { safeReturnTo } from "./returnTo";

type AuthStatus = "loading" | "signed_out" | "ready" | "error";
interface AuthContextValue {
  status: AuthStatus;
  session: CurrentSession | null;
  error: string | null;
  demoUsers: DemoUser[];
  mode: "dev" | "entra" | null;
  login(returnTo?: string): Promise<void>;
  logout(): Promise<void>;
  selectDemoUser(userId: string): Promise<void>;
  refreshSession(): Promise<CurrentSession>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [adapter, setAdapter] = useState<AuthAdapter | null>(null);
  const [session, setSession] = useState<CurrentSession | null>(null);
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [error, setError] = useState<string | null>(null);
  const [demoUsers, setDemoUsers] = useState<DemoUser[]>([]);

  const refreshSession = useCallback(async () => {
    const current = await apiRequest<CurrentSession>("/me");
    if (!current.user || !current.user.id || !current.user.display_name) {
      throw new Error("社員プロフィールを取得できませんでした。打刻は行われていません。");
    }
    setSession(current);
    setStatus("ready");
    setError(null);
    return current;
  }, []);

  useEffect(() => {
    let alive = true;
    void createAuthAdapter().then(async (created) => {
      if (!alive) return;
      setAdapter(created);
      configureApiCredentials({ accessToken: () => created.getAccessToken(), devUserId: () => created.getDevUserId() });
      if (created.mode === "dev") {
        try {
          const users = await apiRequest<DemoUser[]>("/dev/users");
          if (alive) setDemoUsers(users);
        } catch (cause) {
          if (alive) {
            setError(cause instanceof Error ? cause.message : "開発用ユーザーを取得できませんでした。");
            setStatus("error");
          }
          return;
        }
      }
      if (!created.hasAccount()) {
        if (alive) setStatus("signed_out");
        return;
      }
      try {
        await refreshSession();
      } catch {
        if (alive) {
          setSession(null);
          setStatus("error");
          setError("認証後の社員プロフィール取得に失敗しました。出退勤記録は作成されていません。");
        }
      }
    }).catch((cause: unknown) => {
      if (alive) {
        setStatus("error");
        setError(cause instanceof Error ? cause.message : "認証の初期化に失敗しました。");
      }
    });
    return () => { alive = false; };
  }, [refreshSession]);

  const login = useCallback(async (returnTo?: string) => {
    if (!adapter) return;
    const destination = safeReturnTo(returnTo ?? `${location.pathname}${location.search}`, window.location.origin);
    setError(null);
    try {
      await adapter.login(destination);
      if (adapter.mode === "dev" && !adapter.hasAccount()) return;
      await refreshSession();
      navigate(destination, { replace: true });
    } catch (cause) {
      setStatus("error");
      setError(cause instanceof Error ? cause.message : "ログインまたはプロフィール取得に失敗しました。");
    }
  }, [adapter, location.pathname, location.search, navigate, refreshSession]);

  const logout = useCallback(async () => {
    await adapter?.logout();
    setSession(null);
    setStatus("signed_out");
    navigate("/attendance", { replace: true });
  }, [adapter, navigate]);

  const selectDemoUser = useCallback(async (userId: string) => {
    if (!adapter || adapter.mode !== "dev") return;
    adapter.selectDevUser(userId);
    setStatus("loading");
    try {
      await refreshSession();
      const routeState = location.state as { returnTo?: string } | null;
      const returnTo = routeState?.returnTo ?? `${location.pathname}${location.search}`;
      const destination = safeReturnTo(returnTo, window.location.origin);
      navigate(destination, { replace: true });
    } catch (cause) {
      setSession(null);
      setStatus("error");
      setError(cause instanceof Error ? cause.message : "選択した社員プロフィールを取得できませんでした。");
    }
  }, [adapter, location.pathname, location.search, navigate, refreshSession]);

  const value = useMemo<AuthContextValue>(() => ({
    status, session, error, demoUsers, mode: adapter?.mode ?? null, login, logout, selectDemoUser, refreshSession,
  }), [adapter?.mode, demoUsers, error, login, logout, refreshSession, selectDemoUser, session, status]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (!value) throw new Error("useAuth must be used inside AuthProvider");
  return value;
}
