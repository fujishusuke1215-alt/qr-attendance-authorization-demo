import { Route, Routes } from "react-router-dom";
import { RequireAdmin, RequireAuth } from "../auth/guards";
import { AdminLayout } from "../components/AdminLayout";
import { AppLayout } from "../components/AppLayout";
import { AttendancePage } from "../features/attendance/AttendancePage";
import { ScanPage } from "../features/attendance/ScanPage";
import { HrSyncPage } from "../features/hr-sync/HrSyncPage";
import { PermissionsPage } from "../features/permissions/PermissionsPage";
import { QrAdminPage } from "../features/qr/QrAdminPage";
import { ForbiddenPage, LoginPage, NotFoundPage } from "../pages/StaticPages";

export function AppRoutes() {
  return <Routes>
    <Route path="/login" element={<LoginPage />} />
    <Route path="/forbidden" element={<ForbiddenPage />} />
    <Route element={<RequireAuth />}>
      <Route element={<AppLayout />}>
        <Route path="/attendance" element={<AttendancePage />} />
        <Route path="/scan/:token" element={<ScanPage />} />
        <Route element={<RequireAdmin />}>
          <Route element={<AdminLayout />}>
            <Route path="/admin/qr" element={<QrAdminPage />} />
            <Route path="/admin/permissions" element={<PermissionsPage />} />
            <Route path="/admin/hr-sync" element={<HrSyncPage />} />
          </Route>
        </Route>
      </Route>
    </Route>
    <Route path="*" element={<NotFoundPage />} />
  </Routes>;
}
