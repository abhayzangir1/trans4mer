import { useEffect, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { FolderOpen, FolderPlus, X, AlertCircle, CheckCircle, AlertTriangle, Info } from 'lucide-react';
import WorkspaceShell from './components/layout/WorkspaceShell';
import SetupWizard from './components/modals/SetupWizard';
import SettingsModal from './components/settings/SettingsModal';
import { useProjectStore } from './store/projectStore';
import { useUiStore } from './store/uiStore';

function App() {
  const isRunningInTauri = typeof window !== 'undefined' && (isTauri() || (window as any).__TAURI_INTERNALS__ !== undefined);
  const [dbStatus, setDbStatus] = useState<string>(isRunningInTauri ? 'Connecting...' : 'Desktop App Running');
  const [ollamaStatus, setOllamaStatus] = useState<string>(isRunningInTauri ? 'Checking...' : 'Desktop Mode');
  const [isSetupComplete, setIsSetupComplete] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isNewProjectOpen, setIsNewProjectOpen] = useState(false);
  const [projectName, setProjectName] = useState('');
  const [projectPath, setProjectPath] = useState('');
  const [creatingProject, setCreatingProject] = useState(false);
  const [modalError, setModalError] = useState<string | null>(null);

  const { projects, activeProjectId, fetchProjects, createProject, deleteProject, setActiveProject } = useProjectStore();
  const { toasts, removeToast, showToast } = useUiStore();

  const handlePickDirectory = async () => {
    setModalError(null);
    try {
      const selected = await invoke<string | null>('pick_directory');
      if (selected) {
        setProjectPath(selected);
        if (!projectName.trim()) {
          const parts = selected.replace(/[\\/]+$/, '').split(/[\\/]/);
          const folderName = parts[parts.length - 1];
          if (folderName) {
            setProjectName(folderName);
          }
        }
        setIsNewProjectOpen(true);
      }
    } catch (err: any) {
      console.error("Failed to pick directory:", err);
      setModalError(`Directory selection failed: ${err?.message || err}`);
    }
  };

  useEffect(() => {
    if (!isRunningInTauri) {
      setDbStatus('Desktop Shell Required');
      setOllamaStatus('Unavailable in Web Browser');
      setIsSetupComplete(true);
      return;
    }
    const checkConfigurationAndStatus = async () => {
      try {
        const [settings, status] = await Promise.all([
          invoke<{ provider: string; keys: Array<{ provider: string; is_set: boolean }> }>('get_settings'),
          invoke<{ database_status: string; ollama_status: string }>('cmd_get_system_status')
        ]);
        setDbStatus(status.database_status);
        setOllamaStatus(status.ollama_status);

        const hasCloudKey = settings.keys?.some(k => k.is_set);
        const isOllamaOnline = status.ollama_status.toLowerCase().includes('online');
        if (hasCloudKey || isOllamaOnline) {
          setIsSetupComplete(true);
        }
      } catch (err) {
        console.error("Failed to initialize system state:", err);
        setDbStatus(`Error: ${err}`);
        setOllamaStatus('Offline');
      }
    };

    checkConfigurationAndStatus();
    const interval = setInterval(() => {
      invoke<{ database_status: string; ollama_status: string }>('cmd_get_system_status')
        .then((status) => {
          setDbStatus(status.database_status);
          setOllamaStatus(status.ollama_status);
        })
        .catch((err) => {
          setDbStatus(`Error: ${err}`);
          setOllamaStatus('Offline');
        });
    }, 10000);

    fetchProjects();

    return () => clearInterval(interval);
  }, [fetchProjects, isRunningInTauri]);

  // Auto-select first project if available and none selected
  useEffect(() => {
    if (projects.length > 0 && !activeProjectId) {
      setActiveProject(projects[0].id);
    }
  }, [projects, activeProjectId, setActiveProject]);

  // Auto-prompt workspace picker if setup is ready and no projects exist
  const [hasPromptedWorkspace, setHasPromptedWorkspace] = useState(false);
  useEffect(() => {
    if (isSetupComplete && projects.length === 0 && !hasPromptedWorkspace) {
      setHasPromptedWorkspace(true);
      setIsNewProjectOpen(true);
    }
  }, [isSetupComplete, projects.length, hasPromptedWorkspace]);

  const handleCreateProject = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!projectName.trim() || !projectPath.trim()) return;
    setCreatingProject(true);
    setModalError(null);
    try {
      await createProject(projectName.trim(), projectPath.trim());
      setProjectName('');
      setProjectPath('');
      setIsNewProjectOpen(false);
    } catch (err: any) {
      console.error("Failed to create project:", err);
      setModalError(`Failed to create project: ${err?.message || err}`);
    } finally {
      setCreatingProject(false);
    }
  };

  const activeProject = projects.find(p => p.id === activeProjectId);

  return (
    <div className="h-full w-full flex flex-col relative bg-bg-base text-text-primary">

      {!isSetupComplete && (
        <SetupWizard onComplete={() => setIsSetupComplete(true)} />
      )}

      <SettingsModal isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />

      {/* New Project Modal */}
      {isNewProjectOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-[480px] rounded-lg border border-gray-800 bg-bg-surface p-6 shadow-2xl">
            <h2 className="text-base font-bold text-gray-100 mb-4 flex items-center space-x-2">
              <FolderPlus size={18} className="text-brand-primary" />
              <span>Create / Open Project Workspace</span>
            </h2>
            {modalError && (
              <div className="p-2.5 rounded bg-red-500/10 border border-red-500/30 text-red-400 text-xs font-mono mb-3">
                {modalError}
              </div>
            )}
            <form onSubmit={handleCreateProject} className="space-y-4">
              <div>
                <label className="block text-xs font-mono text-gray-400 mb-1">Workspace Directory Path</label>
                <div className="flex space-x-2">
                  <input
                    type="text"
                    value={projectPath}
                    onChange={(e) => setProjectPath(e.target.value)}
                    placeholder="Click Browse or enter directory path..."
                    required
                    className="flex-1 rounded border border-gray-700 bg-bg-base px-3 py-2 text-xs font-mono text-gray-200 focus:border-brand-primary focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={handlePickDirectory}
                    className="flex items-center space-x-1.5 px-3 py-2 rounded bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700 text-xs font-mono transition-colors"
                  >
                    <FolderOpen size={14} className="text-brand-primary" />
                    <span>Browse...</span>
                  </button>
                </div>
              </div>
              <div>
                <label className="block text-xs font-mono text-gray-400 mb-1">Project Name</label>
                <input
                  type="text"
                  value={projectName}
                  onChange={(e) => setProjectName(e.target.value)}
                  placeholder="e.g. My Autonomous App"
                  required
                  className="w-full rounded border border-gray-700 bg-bg-base px-3 py-2 text-xs font-mono text-gray-200 focus:border-brand-primary focus:outline-none"
                />
              </div>
              <div className="flex justify-end space-x-2 pt-2">
                <button
                  type="button"
                  onClick={() => setIsNewProjectOpen(false)}
                  className="rounded px-3 py-1.5 text-xs font-mono text-gray-400 hover:text-gray-200"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={creatingProject}
                  className="rounded bg-brand-primary px-4 py-1.5 text-xs font-mono font-medium text-black hover:opacity-90 disabled:opacity-50"
                >
                  {creatingProject ? 'Opening...' : 'Open Workspace'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Top Application Bar */}
      <div className="h-9 border-b border-gray-800 bg-bg-surface flex items-center px-4 text-xs font-mono text-gray-400 justify-between select-none">
        <div className="flex items-center space-x-4">
          <div className="flex items-center space-x-2 font-semibold text-gray-200">
            <span className="text-brand-primary">●</span>
            <span>Trans4mers OS</span>
          </div>

          <div className="h-4 w-[1px] bg-gray-700" />

          {/* Project Picker */}
          <div className="flex items-center space-x-2">
            <span className="text-gray-500">Project:</span>
            {projects.length > 0 ? (
              <select
                value={activeProjectId || ''}
                onChange={(e) => setActiveProject(e.target.value)}
                className="bg-bg-base border border-gray-700 text-gray-200 rounded px-2 py-0.5 text-xs font-mono focus:border-brand-primary focus:outline-none"
              >
                {projects.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            ) : (
              <span className="text-gray-500 italic">No projects created</span>
            )}
            <button
              onClick={() => setIsNewProjectOpen(true)}
              className="px-2 py-0.5 rounded bg-gray-800 hover:bg-gray-700 text-gray-300 text-xs font-mono transition-colors"
            >
              + New
            </button>
            {activeProjectId && activeProject && (
              <button
                onClick={async () => {
                  const ok = window.confirm(`Delete project "${activeProject.name}" from Trans4mers? (Workspace files on disk will not be deleted)`);
                  if (ok) {
                    try {
                      await deleteProject(activeProjectId);
                      showToast('info', `Project "${activeProject.name}" removed`);
                    } catch (err: any) {
                      showToast('error', `Failed to delete project: ${err?.message || err}`);
                    }
                  }
                }}
                className="px-1.5 py-0.5 rounded bg-red-900/20 hover:bg-red-900/40 text-red-400 text-xs font-mono transition-colors"
                title="Delete project configuration"
              >
                ✕
              </button>
            )}
          </div>
        </div>

        <div className="flex items-center space-x-4">
          {activeProject && (
            <span className="text-gray-500 truncate max-w-[200px]" title={activeProject.workspace_path}>
              {activeProject.workspace_path}
            </span>
          )}
          <span className="flex items-center space-x-1.5" title="Local LLM Engine Status">
            <span className={`inline-block w-2 h-2 rounded-full ${ollamaStatus === 'Online' ? 'bg-green-500' : 'bg-red-500 animate-pulse'}`} />
            <span>Ollama: {ollamaStatus}</span>
          </span>
          <span className="flex items-center space-x-1.5" title="SQLite Database Status">
            <span className={`inline-block w-2 h-2 rounded-full ${dbStatus === 'Connected' ? 'bg-green-500' : 'bg-yellow-500'}`} />
            <span>DB: {dbStatus}</span>
          </span>
          <button
            onClick={() => setIsSettingsOpen(true)}
            className="hover:text-gray-200 text-gray-400 transition-colors"
            title="Open System Settings"
          >
            ⚙ Settings
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-hidden">
        {projects.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center bg-bg-base p-8 text-center select-none font-mono">
            <div className="w-16 h-16 rounded-2xl bg-brand-primary/10 border border-brand-primary/30 flex items-center justify-center mb-6">
              <FolderOpen size={32} className="text-brand-primary" />
            </div>
            <h1 className="text-xl font-bold text-gray-100 mb-2">Welcome to Trans4mers Sovereign Agent OS</h1>
            <p className="text-sm text-gray-400 max-w-md mb-6 leading-relaxed">
              To begin, select a local folder or repository as your autonomous workspace. Your agent team will inspect files, execute tasks, and collaborate directly in this directory.
            </p>
            <button
              onClick={handlePickDirectory}
              className="flex items-center space-x-2 px-5 py-2.5 rounded-lg bg-brand-primary hover:bg-brand-primary/90 text-black font-semibold text-xs shadow-lg transition-all"
            >
              <FolderOpen size={16} />
              <span>Choose Workspace Directory</span>
            </button>
          </div>
        ) : (
          <WorkspaceShell />
        )}
      </div>

      {/* Global Toast Notifications */}
      <div className="fixed bottom-6 right-6 z-50 flex flex-col space-y-2 max-w-md w-full pointer-events-none">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`pointer-events-auto flex items-start space-x-3 p-3.5 rounded-lg shadow-xl border text-xs font-mono backdrop-blur-md transition-all ${
              t.type === 'error'
                ? 'bg-red-950/90 border-red-500/50 text-red-200'
                : t.type === 'warning'
                ? 'bg-yellow-950/90 border-yellow-500/50 text-yellow-200'
                : t.type === 'success'
                ? 'bg-green-950/90 border-green-500/50 text-green-200'
                : 'bg-gray-900/90 border-gray-700 text-gray-200'
            }`}
          >
            <div className="mt-0.5 shrink-0">
              {t.type === 'error' && <AlertCircle size={15} className="text-red-400" />}
              {t.type === 'warning' && <AlertTriangle size={15} className="text-yellow-400" />}
              {t.type === 'success' && <CheckCircle size={15} className="text-green-400" />}
              {t.type === 'info' && <Info size={15} className="text-blue-400" />}
            </div>
            <div className="flex-1 break-words leading-relaxed">{t.message}</div>
            <button
              onClick={() => removeToast(t.id)}
              className="text-gray-400 hover:text-gray-100 transition-colors p-0.5"
            >
              <X size={13} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

export default App;
