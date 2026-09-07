import { useState, useEffect, useRef } from 'react';
import Editor from '@monaco-editor/react';
import { invoke } from '@tauri-apps/api/core';
import { useProjectStore } from '../../store/projectStore';
import { useUiStore } from '../../store/uiStore';

interface Props {
  filePath?: string | null;
}

function detectLanguage(filePath?: string | null): string {
  if (!filePath) return 'plaintext';
  const ext = filePath.split('.').pop()?.toLowerCase() || '';
  switch (ext) {
    case 'rs':
      return 'rust';
    case 'ts':
    case 'tsx':
      return 'typescript';
    case 'js':
    case 'jsx':
      return 'javascript';
    case 'json':
      return 'json';
    case 'md':
      return 'markdown';
    case 'toml':
      return 'toml';
    case 'css':
      return 'css';
    case 'html':
      return 'html';
    case 'sql':
      return 'sql';
    case 'sh':
    case 'bash':
      return 'shell';
    case 'py':
      return 'python';
    case 'yaml':
    case 'yml':
      return 'yaml';
    default:
      return 'plaintext';
  }
}

export default function CodeEditor({ filePath }: Props) {
  const [content, setContent] = useState<string>('// Select a file to view its contents...');
  const [originalContent, setOriginalContent] = useState<string>('');
  const [isSaving, setIsSaving] = useState(false);
  const [lastSaved, setLastSaved] = useState<Date | null>(null);
  const activeProjectId = useProjectStore(state => state.activeProjectId);

  const isDirty = Boolean(filePath) && content !== originalContent;

  const contentRef = useRef(content);
  const filePathRef = useRef(filePath);
  const activeProjectIdRef = useRef(activeProjectId);
  const isSavingRef = useRef(isSaving);

  useEffect(() => { contentRef.current = content; }, [content]);
  useEffect(() => { filePathRef.current = filePath; }, [filePath]);
  useEffect(() => { activeProjectIdRef.current = activeProjectId; }, [activeProjectId]);
  useEffect(() => { isSavingRef.current = isSaving; }, [isSaving]);

  // Fetch physical file content when path changes
  useEffect(() => {
    if (!filePath || !activeProjectId) {
      setContent('// Select a file to view its contents...');
      setOriginalContent('');
      return;
    }

    let isMounted = true;
    invoke<string>('read_file', { projectId: activeProjectId, path: filePath })
      .then((data) => {
        if (isMounted) {
          setContent(data);
          setOriginalContent(data);
        }
      })
      .catch((err) => {
        if (isMounted) {
          setContent(`// Error reading file: ${err}`);
          setOriginalContent('');
        }
      });

    return () => { isMounted = false; };
  }, [filePath, activeProjectId]);

  const saveFile = async () => {
    const curPath = filePathRef.current;
    const curProj = activeProjectIdRef.current;
    const curContent = contentRef.current;
    if (!curPath || !curProj || isSavingRef.current) return;
    setIsSaving(true);
    try {
      await invoke('write_file', { projectId: curProj, path: curPath, content: curContent });
      setOriginalContent(curContent);
      setLastSaved(new Date());
      useUiStore.getState().showToast('success', `Saved ${curPath}`);
    } catch (err: any) {
      console.error("Failed to save file", err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to save file');
    } finally {
      setIsSaving(false);
    }
  };

  // Handle Ctrl+S / Cmd+S save action
  const handleEditorDidMount = (editor: any, monaco: any) => {
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      saveFile();
    });
  };

  const detectedLang = detectLanguage(filePath);

  return (
    <div className="h-full w-full relative flex flex-col bg-gray-950">
      {filePath && (
        <div className="h-8 px-3 bg-gray-900 border-b border-gray-800 flex items-center justify-between text-[11px] text-gray-400 font-mono select-none">
          <div className="flex items-center space-x-2 truncate">
            <span className="truncate text-gray-200">{filePath}</span>
            {isDirty && (
              <span className="w-2 h-2 rounded-full bg-amber-400" title="Unsaved changes" />
            )}
          </div>
          <div className="flex items-center space-x-3">
            {isSaving ? (
              <span className="text-amber-400 text-[10px]">Saving & Syncing...</span>
            ) : lastSaved ? (
              <span className="text-emerald-400 text-[10px] flex items-center space-x-1" title="Changes emitted to Agent event stream">
                <span>✨ AI Aware • Synced</span>
              </span>
            ) : null}

            {isDirty && (
              <button
                onClick={saveFile}
                className="bg-blue-600 hover:bg-blue-500 text-white px-2 py-0.5 rounded text-[10px] transition-colors"
              >
                Save (Ctrl+S)
              </button>
            )}

            <span className="uppercase text-[10px] bg-gray-800 px-1.5 py-0.5 rounded text-gray-300">
              {detectedLang}
            </span>
          </div>
        </div>
      )}
      <div className="flex-1 relative">
        <Editor
          height="100%"
          language={detectedLang}
          theme="vs-dark"
          value={content}
          onChange={(val) => setContent(val || '')}
          onMount={handleEditorDidMount}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            fontFamily: 'Menlo, Monaco, "Courier New", monospace',
            padding: { top: 12 },
            automaticLayout: true,
          }}
        />
      </div>
    </div>
  );
}
