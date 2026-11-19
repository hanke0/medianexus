import { createBrowserRouter, Navigate, RouterProvider } from "react-router-dom";
import { ProtectedRoute } from "@/components/ProtectedRoute";
import { AppLayout } from "@/components/AppLayout";
import { LoginPage } from "@/pages/LoginPage";
import { BrowsePage } from "@/pages/BrowsePage";
import { TracksPage } from "@/pages/TracksPage";
import { AdminUsersPage } from "@/pages/AdminUsersPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { MediaDetailPage } from "@/pages/MediaDetailPage";
import { MaintenancePage } from "@/pages/MaintenancePage";

const router = createBrowserRouter([
  { path: "/login", element: <LoginPage /> },
  {
    element: (
      <ProtectedRoute>
        <AppLayout />
      </ProtectedRoute>
    ),
    children: [
      { path: "/", element: <BrowsePage /> },
      { path: "/media/:mediaType/:tmdbId", element: <MediaDetailPage /> },
      { path: "/tracks", element: <TracksPage /> },
      { path: "/admin/users", element: <AdminUsersPage /> },
      { path: "/admin/settings", element: <SettingsPage /> },
      { path: "/admin/maintenance", element: <MaintenancePage /> },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
]);

export default function App() {
  return <RouterProvider router={router} />;
}
