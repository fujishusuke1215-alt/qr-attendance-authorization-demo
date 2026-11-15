const SAFE_FIXED_PATHS = new Set(["/attendance", "/admin/qr", "/admin/permissions", "/admin/hr-sync"]);

export function safeReturnTo(candidate: string | null | undefined, origin = window.location.origin): string {
  if (!candidate || !candidate.startsWith("/") || candidate.startsWith("//") || candidate.includes("\\") || candidate.includes("\0")) {
    return "/attendance";
  }
  let target: URL;
  try {
    target = new URL(candidate, origin);
  } catch {
    return "/attendance";
  }
  if (target.origin !== origin) return "/attendance";
  if (SAFE_FIXED_PATHS.has(target.pathname)) return `${target.pathname}${target.search}${target.hash}`;
  if (/^\/scan\/[A-Za-z0-9._-]+$/.test(target.pathname)) return target.pathname;
  return "/attendance";
}

export function hasSafeAttendanceProfile(profile: unknown): boolean {
  if (typeof profile !== "object" || profile === null) return false;
  const value = profile as Record<string, unknown>;
  return typeof value.id === "string"
    && value.id.length > 0
    && typeof value.display_name === "string"
    && value.display_name.trim().length > 0
    && typeof value.employee_code === "string"
    && value.employee_code.trim().length > 0
    && typeof value.email === "string"
    && value.email.includes("@");
}
