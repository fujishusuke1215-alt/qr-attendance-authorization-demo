import { apiRequest, jsonBody } from "./client";
import type { AttendanceRecord, TodayResponse } from "./types";

export function getToday(): Promise<TodayResponse> {
  return apiRequest("/attendance/today");
}

export function createAttendance(token: string, idempotencyKey: string): Promise<{ created: boolean; event: AttendanceRecord }> {
  return apiRequest("/attendance/scan", {
    method: "POST",
    headers: { "Idempotency-Key": idempotencyKey },
    body: jsonBody({ token }),
  });
}

export function getScopedAttendance(date?: string): Promise<AttendanceRecord[]> {
  const query = date ? `?date=${encodeURIComponent(date)}` : "";
  return apiRequest(`/attendance/scope${query}`);
}
