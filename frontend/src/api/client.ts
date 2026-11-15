const apiBase = import.meta.env.VITE_API_BASE_URL || "/api";

export interface ApiErrorBody { error?: { code?: string; message?: string } }

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

type Credentials = { accessToken?: () => Promise<string | null>; devUserId?: () => string | null };
let credentials: Credentials = {};

export function configureApiCredentials(value: Credentials): void {
  credentials = value;
}

export async function apiRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  const token = await credentials.accessToken?.();
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const devUserId = credentials.devUserId?.();
  if (devUserId) headers.set("X-Dev-User", devUserId);

  let response: Response;
  try {
    response = await fetch(`${apiBase}${path}`, { ...init, headers });
  } catch {
    throw new ApiError(0, "network_error", "サーバーに接続できません。起動状態を確認してください。");
  }
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as ApiErrorBody | null;
    throw new ApiError(response.status, body?.error?.code ?? "api_error", body?.error?.message ?? "リクエストに失敗しました。");
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function jsonBody(value: unknown): string {
  return JSON.stringify(value);
}
