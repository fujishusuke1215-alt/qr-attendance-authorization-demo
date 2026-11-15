import { PublicClientApplication, type AccountInfo, type IPublicClientApplication } from "@azure/msal-browser";

export type AuthMode = "dev" | "entra";

export interface AuthAdapter {
  mode: AuthMode;
  hasAccount(): boolean;
  getAccessToken(): Promise<string | null>;
  login(returnTo: string): Promise<void>;
  logout(): Promise<void>;
  selectDevUser(userId: string): void;
  getDevUserId(): string | null;
}

class DevelopmentAdapter implements AuthAdapter {
  readonly mode = "dev" as const;
  hasAccount(): boolean { return Boolean(this.getDevUserId()); }
  async getAccessToken(): Promise<null> { return null; }
  async login(): Promise<void> { /* A demo identity is selected on the sign-in screen. */ }
  async logout(): Promise<void> { localStorage.removeItem("shiftline.demo-user"); }
  selectDevUser(userId: string): void { localStorage.setItem("shiftline.demo-user", userId); }
  getDevUserId(): string | null { return localStorage.getItem("shiftline.demo-user"); }
}

class EntraAdapter implements AuthAdapter {
  readonly mode = "entra" as const;
  private account: AccountInfo | null;

  constructor(private readonly client: IPublicClientApplication, private readonly scope: string, account: AccountInfo | null) {
    this.account = account;
  }

  hasAccount(): boolean { return this.account !== null; }

  async getAccessToken(): Promise<string | null> {
    if (!this.account) return null;
    try {
      const result = await this.client.acquireTokenSilent({ account: this.account, scopes: [this.scope] });
      return result.accessToken;
    } catch {
      return null;
    }
  }

  async login(_returnTo: string): Promise<void> {
    const result = await this.client.loginPopup({ scopes: [this.scope] });
    this.account = result.account;
    if (result.account) this.client.setActiveAccount(result.account);
  }

  async logout(): Promise<void> {
    if (this.account) await this.client.logoutPopup({ account: this.account });
    this.account = null;
  }

  selectDevUser(): void { /* No local identities exist in the Entra adapter. */ }
  getDevUserId(): null { return null; }
}

const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export async function createAuthAdapter(): Promise<AuthAdapter> {
  const mode = (import.meta.env.VITE_AUTH_MODE || "dev") as AuthMode;
  if (mode === "dev") return new DevelopmentAdapter();
  if (mode !== "entra") throw new Error("VITE_AUTH_MODE は dev または entra を指定してください。");

  const tenantId = import.meta.env.VITE_ENTRA_TENANT_ID || "";
  const clientId = import.meta.env.VITE_ENTRA_CLIENT_ID || "";
  const scope = import.meta.env.VITE_ENTRA_API_SCOPE || "";
  if (!guid.test(tenantId) || !guid.test(clientId) || !scope.startsWith("api://") || !scope.endsWith("/access_as_user")) {
    throw new Error("Entra ID のテナント、クライアント、API scope を設定してください。");
  }

  const client = new PublicClientApplication({
    auth: {
      clientId,
      authority: `https://login.microsoftonline.com/${tenantId}`,
      redirectUri: window.location.origin,
    },
    cache: { cacheLocation: "sessionStorage" },
  });
  await client.initialize();
  const redirectResult = await client.handleRedirectPromise();
  const account = redirectResult?.account ?? client.getActiveAccount() ?? client.getAllAccounts()[0] ?? null;
  if (account) client.setActiveAccount(account);
  return new EntraAdapter(client, scope, account);
}
