import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { ProtectedRoute } from "@/components/ProtectedRoute";
import { AppLayout } from "@/components/AppLayout";
import { LoginPage } from "@/pages/LoginPage";
import { BrowsePage } from "@/pages/BrowsePage";
import { TracksPage } from "@/pages/TracksPage";
import { AdminUsersPage } from "@/pages/AdminUsersPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { MediaDetailPage } from "@/pages/MediaDetailPage";
import { MaintenancePage } from "@/pages/MaintenancePage";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route
          element={
            <ProtectedRoute>
              <AppLayout />
            </ProtectedRoute>
          }
        >
          <Route path="/" element={<BrowsePage />} />
          <Route path="/media/:mediaType/:tmdbId" element={<MediaDetailPage />} />
          <Route path="/tracks" element={<TracksPage />} />
          <Route path="/admin/users" element={<AdminUsersPage />} />
          <Route path="/admin/settings" element={<SettingsPage />} />
          <Route path="/admin/maintenance" element={<MaintenancePage />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
