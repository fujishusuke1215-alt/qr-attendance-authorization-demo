import { hasSafeAttendanceProfile } from "../../auth/returnTo";

export type ProfileProvider = () => Promise<unknown>;
export type AttendanceWriter<T> = (token: string, idempotencyKey: string) => Promise<T>;

const keyPrefix = "shiftline.attendance.idempotency:";

export function getOrCreateIdempotencyKey(token: string, storage: Pick<Storage, "getItem" | "setItem"> = sessionStorage): string {
  const key = `${keyPrefix}${token}`;
  const existing = storage.getItem(key);
  if (existing) return existing;
  const created = crypto.randomUUID();
  storage.setItem(key, created);
  return created;
}

/** A profile lookup must complete before the punch request is sent. */
export async function submitAttendanceAfterProfile<T>(
  token: string,
  idempotencyKey: string,
  loadProfile: ProfileProvider,
  createAttendance: AttendanceWriter<T>,
): Promise<T> {
  const profile = await loadProfile();
  if (!hasSafeAttendanceProfile(profile)) throw new Error("プロフィールに氏名、社員番号、メールアドレスが揃っていません。打刻は記録されていません。");
  return createAttendance(token, idempotencyKey);
}
