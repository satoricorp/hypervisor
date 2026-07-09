import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { MainPane } from "./components/MainPane";
import { PromptBar } from "./components/PromptBar";
import { Statusbar } from "./components/Statusbar";
import { Toast } from "./components/Toast";
import { Palette } from "./components/Palette";
import { useKeyboard } from "./hooks/useKeyboard";

function AppShell() {
  useKeyboard();
  return (
    <>
      <div className="window" role="application" aria-label="Hypervisor">
        <Titlebar />
        <div className="split">
          <Sidebar />
          <div className="rightcol">
            <MainPane />
            <PromptBar />
          </div>
        </div>
        <Statusbar />
      </div>
      <Toast />
      <Palette />
    </>
  );
}

export default function App() {
  return <AppShell />;
}
