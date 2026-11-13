import "./index.css";
import { render } from "preact";
import { useSignal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import { AdminDashboard } from "./pages/AdminDashboard";
import { UserPortal } from "./pages/UserPortal";

function App() {
  const pathname = useSignal(window.location.pathname);

  useEffect(() => {
    const onPopState = () => {
      pathname.value = window.location.pathname;
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  function navigate(nextPath: string) {
    if (window.location.pathname !== nextPath) {
      window.history.pushState({}, "", nextPath);
      pathname.value = nextPath;
      window.scrollTo({ top: 0, behavior: "smooth" });
    }
  }

  const isAdmin = pathname.value.startsWith("/admin");
  return isAdmin ? <AdminDashboard onNavigate={navigate} /> : <UserPortal onNavigate={navigate} />;
}

const root = document.getElementById("app");
if (root) {
  render(<App />, root);
}
