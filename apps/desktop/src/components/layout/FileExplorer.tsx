import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Folder, FolderOpen, FileText, ChevronRight, ChevronDown, RefreshCw } from 'lucide-react';
import { useUiStore } from '../../store/uiStore';

interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
}

interface TreeNodeProps {
  node: FileNode;
  projectId: string;
  depth: number;
  selectedFilePath: string | null;
  onSelectFile: (path: string) => void;
}

function TreeNode({ node, projectId, depth, selectedFilePath, onSelectFile }: TreeNodeProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [children, setChildren] = useState<FileNode[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const toggleOpen = async () => {
    if (!node.is_dir) {
      onSelectFile(node.path);
      return;
    }

    if (!isOpen && children.length === 0) {
      setIsLoading(true);
      try {
        const subNodes: FileNode[] = await invoke('list_directory', {
          projectId,
          path: node.path,
        });
        setChildren(subNodes);
      } catch (err) {
        console.error(`Failed to list directory ${node.path}:`, err);
      } finally {
        setIsLoading(false);
      }
    }
    setIsOpen(!isOpen);
  };

  const isSelected = selectedFilePath === node.path;

  return (
    <div>
      <div
        onClick={toggleOpen}
        style={{ paddingLeft: `${depth * 14 + 6}px` }}
        className={`flex items-center space-x-1.5 py-1 pr-2 rounded cursor-pointer select-none transition-colors ${
          isSelected
            ? 'bg-brand-primary/20 text-brand-primary font-medium'
            : 'hover:bg-gray-800 text-gray-400 hover:text-gray-200'
        }`}
      >
        {node.is_dir ? (
          <>
            <span className="text-gray-500 w-3.5 flex items-center justify-center">
              {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            </span>
            {isOpen ? (
              <FolderOpen size={14} className="text-blue-400 shrink-0" />
            ) : (
              <Folder size={14} className="text-blue-400 shrink-0" />
            )}
          </>
        ) : (
          <>
            <span className="w-3.5" />
            <FileText size={14} className="text-gray-400 shrink-0" />
          </>
        )}
        <span className="truncate">{node.name}</span>
        {isLoading && <span className="text-[10px] text-gray-500 animate-pulse">...</span>}
      </div>

      {node.is_dir && isOpen && (
        <div>
          {children.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              projectId={projectId}
              depth={depth + 1}
              selectedFilePath={selectedFilePath}
              onSelectFile={onSelectFile}
            />
          ))}
          {children.length === 0 && !isLoading && (
            <div
              style={{ paddingLeft: `${(depth + 1) * 14 + 18}px` }}
              className="py-0.5 text-gray-600 italic text-[11px]"
            >
              (empty)
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function FileExplorer() {
  const [files, setFiles] = useState<FileNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [gitStatus, setGitStatus] = useState<string | null>(null);
  const { activeProjectId, selectedFilePath, setSelectedFile } = useUiStore();
  const [rootPath, setRootPath] = useState<string | null>(null);

  const loadGitStatus = async () => {
    if (!activeProjectId) return;
    try {
      const status: string = await invoke('get_git_status', { projectId: activeProjectId });
      setGitStatus(status);
    } catch {
      setGitStatus(null);
    }
  };

  const loadFiles = async () => {
    if (!activeProjectId) return;
    setLoading(true);
    try {
      const nodes: FileNode[] = await invoke('list_directory', {
        projectId: activeProjectId,
        path: '',
      });
      setFiles(nodes);
    } catch (e) {
      console.error('Failed to read root directory', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!activeProjectId) {
      setRootPath(null);
      setFiles([]);
      setGitStatus(null);
      return;
    }

    async function loadProject() {
      try {
        const proj: any = await invoke('get_project', { projectId: activeProjectId });
        const path = proj?.workspace_path || proj?.workspace_root;
        if (path) {
          setRootPath(path);
        }
      } catch (e) {
        console.error('Failed to load project', e);
      }
    }
    loadProject();
    loadGitStatus();
  }, [activeProjectId]);

  useEffect(() => {
    if (rootPath) {
      loadFiles();
      loadGitStatus();
    }
  }, [rootPath, activeProjectId]);

  return (
    <div className="flex flex-col h-full overflow-hidden text-xs font-mono">
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-800 text-gray-400 font-semibold uppercase tracking-wider text-[10px]">
        <div className="flex items-center space-x-2 truncate">
          <span>Workspace Files</span>
          {gitStatus && (
            <span className="text-[9px] px-1.5 py-0.5 rounded bg-zinc-800 text-emerald-400 font-sans normal-case truncate">
              {gitStatus}
            </span>
          )}
        </div>
        <button
          onClick={() => {
            loadFiles();
            loadGitStatus();
          }}
          title="Refresh files & git status"
          className="hover:text-gray-200 transition-colors p-0.5 rounded hover:bg-gray-800"
        >
          <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-1.5 space-y-0.5">
        {files.map((file) => (
          <TreeNode
            key={file.path}
            node={file}
            projectId={activeProjectId || ''}
            depth={0}
            selectedFilePath={selectedFilePath}
            onSelectFile={setSelectedFile}
          />
        ))}
        {files.length === 0 && !loading && (
          <div className="p-3 text-gray-600 italic">No files found.</div>
        )}
      </div>
    </div>
  );
}
