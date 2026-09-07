import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useProjectStore } from '../../store/projectStore';
import { useUiStore } from '../../store/uiStore';

interface Artifact {
  id: string;
  project_id: string;
  producer_execution_id?: string | null;
  relative_path: string;
  content_hash: string;
  mime_type: string;
  size_bytes: number;
  created_at: string;
}

interface ArtifactComment {
  id: string;
  artifact_id: string;
  user_id: string;
  line_start: number | null;
  line_end: number | null;
  selected_text: string | null;
  comment: string;
  status: string;
  created_at: string;
}

export default function InteractiveArtifactsPanel() {
  const activeProjectId = useProjectStore(state => state.activeProjectId);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState<Artifact | null>(null);
  const [artifactContent, setArtifactContent] = useState<string>('');
  const [comments, setComments] = useState<ArtifactComment[]>([]);
  const [newCommentText, setNewCommentText] = useState('');
  const [selectedSnippet, setSelectedSnippet] = useState('');
  const [targetLine, setTargetLine] = useState<number | ''>('');
  const [isLoading, setIsLoading] = useState(false);
  const [isSending, setIsSending] = useState<string | null>(null);
  const [feedbackMsg, setFeedbackMsg] = useState('');

  const loadArtifacts = async () => {
    if (!activeProjectId) return;
    setIsLoading(true);
    try {
      const list = await invoke<Artifact[]>('get_artifacts', { projectId: activeProjectId });
      setArtifacts(list);
      if (list.length > 0 && !selectedArtifact) {
        setSelectedArtifact(list[0]);
      }
    } catch (err) {
      console.error('Failed to load artifacts', err);
    } finally {
      setIsLoading(false);
    }
  };

  const loadComments = async (artId: string) => {
    if (!activeProjectId) return;
    try {
      const data = await invoke<ArtifactComment[]>('list_artifact_comments', {
        projectId: activeProjectId,
        artifactId: artId,
      });
      setComments(data);
    } catch (err) {
      console.error('Failed to load comments', err);
    }
  };

  const loadContent = async (art: Artifact) => {
    if (!activeProjectId) return;
    try {
      const content = await invoke<string>('read_file', {
        projectId: activeProjectId,
        path: art.relative_path,
      });
      setArtifactContent(content);
    } catch {
      setArtifactContent('// Unable to load raw file contents for this artifact.');
    }
  };

  useEffect(() => {
    setArtifacts([]);
    setSelectedArtifact(null);
    setArtifactContent('');
    setComments([]);

    if (activeProjectId) {
      loadArtifacts();
    }

    const unlisten = listen<string>('domain_event', (event) => {
      try {
        const payload = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload;
        const evType = payload?.event?.type || payload?.type;
        if (evType === 'ArtifactCreated' || evType === 'ArtifactProduced' || evType === 'ExecutionStepCompleted') {
          loadArtifacts();
        }
      } catch (e) {
        console.warn('Failed to parse domain_event in InteractiveArtifactsPanel:', e);
      }
    });

    return () => {
      unlisten.then(f => f());
    };
  }, [activeProjectId]);

  useEffect(() => {
    if (selectedArtifact) {
      loadContent(selectedArtifact);
      loadComments(selectedArtifact.id);
    } else {
      setArtifactContent('');
      setComments([]);
    }
  }, [selectedArtifact]);

  const handleAddComment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProjectId || !selectedArtifact || !newCommentText.trim()) return;

    try {
      await invoke('add_artifact_comment', {
        projectId: activeProjectId,
        artifactId: selectedArtifact.id,
        lineStart: targetLine === '' ? null : Number(targetLine),
        lineEnd: targetLine === '' ? null : Number(targetLine),
        selectedText: selectedSnippet.trim() ? selectedSnippet : null,
        comment: newCommentText.trim(),
        userId: 'Human Developer',
      });
      setNewCommentText('');
      setSelectedSnippet('');
      setTargetLine('');
      loadComments(selectedArtifact.id);
      useUiStore.getState().showToast('info', 'Comment added to artifact');
    } catch (err: any) {
      console.error('Failed to add comment', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to add comment');
    }
  };

  const handleSendToAgent = async (commentId: string) => {
    if (!activeProjectId || !selectedArtifact) return;
    setIsSending(commentId);
    try {
      await invoke('send_artifact_comment_to_agent', {
        projectId: activeProjectId,
        artifactId: selectedArtifact.id,
        commentId,
        targetAgentId: null,
      });
      setFeedbackMsg('Feedback dispatched to Agent inbox!');
      useUiStore.getState().showToast('success', 'Feedback dispatched to Agent inbox!');
      setTimeout(() => setFeedbackMsg(''), 3500);
      loadComments(selectedArtifact.id);
    } catch (err: any) {
      console.error('Failed to send comment to agent', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to dispatch feedback');
    } finally {
      setIsSending(null);
    }
  };

  return (
    <div className="flex h-full w-full bg-gray-950 text-gray-200 text-xs overflow-hidden">
      {/* Sidebar: Artifacts list */}
      <div className="w-64 border-r border-gray-800 flex flex-col">
        <div className="p-3 border-b border-gray-800 flex items-center justify-between">
          <span className="font-semibold text-gray-300">Artifacts & Deliverables</span>
          <button
            onClick={loadArtifacts}
            className="text-gray-400 hover:text-white transition-colors"
            title="Refresh"
          >
            ↻
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {isLoading ? (
            <div className="text-gray-500 p-2">Loading artifacts...</div>
          ) : artifacts.length === 0 ? (
            <div className="text-gray-500 p-4 text-center">
              No artifacts created yet.<br />Run an agent task to generate plans, reports, or code deliverables.
            </div>
          ) : (
            artifacts.map(art => (
              <button
                key={art.id}
                onClick={() => setSelectedArtifact(art)}
                className={`w-full text-left p-2.5 rounded transition-colors ${
                  selectedArtifact?.id === art.id
                    ? 'bg-blue-600/20 text-blue-300 border border-blue-500/30'
                    : 'hover:bg-gray-900 text-gray-400'
                }`}
              >
                <div className="font-medium truncate">{art.relative_path.split('/').pop() || art.relative_path}</div>
                <div className="text-[10px] text-gray-500 truncate mt-0.5">{art.relative_path} ({Math.round(art.size_bytes / 1024)} KB)</div>
              </button>
            ))
          )}
        </div>
      </div>

      {/* Main viewer: Artifact Code/Markdown + Comments */}
      {selectedArtifact ? (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Header */}
          <div className="h-10 px-4 border-b border-gray-800 bg-gray-900/50 flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <span className="font-semibold text-gray-200">{selectedArtifact.relative_path}</span>
              <span className="text-[10px] bg-gray-800 text-gray-400 px-1.5 py-0.5 rounded">{selectedArtifact.mime_type}</span>
            </div>
            {feedbackMsg && (
              <span className="text-emerald-400 text-[11px] animate-fade-in font-medium">
                ✓ {feedbackMsg}
              </span>
            )}
          </div>

          <div className="flex-1 flex overflow-hidden">
            {/* Artifact Content Preview */}
            <div className="flex-1 border-r border-gray-800 overflow-y-auto p-4 font-mono text-[11px] bg-gray-950/80 leading-relaxed select-text">
              <pre className="whitespace-pre-wrap font-mono">{artifactContent}</pre>
            </div>

            {/* Interactive Comment Stream */}
            <div className="w-80 flex flex-col bg-gray-900/40">
              <div className="p-3 border-b border-gray-800 font-semibold text-gray-300 flex items-center justify-between">
                <span>Inline Reviews ({comments.length})</span>
              </div>

              {/* Comments List */}
              <div className="flex-1 overflow-y-auto p-3 space-y-3">
                {comments.length === 0 ? (
                  <div className="text-gray-500 text-center py-6">
                    No review comments yet.<br />Leave instructions or code corrections below.
                  </div>
                ) : (
                  comments.map(c => (
                    <div
                      key={c.id}
                      className="p-2.5 rounded bg-gray-900 border border-gray-800 flex flex-col space-y-1.5"
                    >
                      <div className="flex items-center justify-between text-[10px] text-gray-400">
                        <span className="font-medium text-gray-300">{c.user_id}</span>
                        {c.line_start && (
                          <span className="bg-gray-800 px-1 rounded text-gray-400">Line {c.line_start}</span>
                        )}
                      </div>

                      {c.selected_text && (
                        <div className="bg-gray-950 border-l-2 border-blue-500 pl-2 py-1 text-[10px] text-gray-400 font-mono italic truncate">
                          "{c.selected_text}"
                        </div>
                      )}

                      <div className="text-gray-200">{c.comment}</div>

                      <div className="flex items-center justify-between pt-1 border-t border-gray-800/60">
                        <span className={`text-[9px] uppercase px-1.5 py-0.5 rounded ${
                          c.status === 'dispatched_to_agent'
                            ? 'bg-emerald-950 text-emerald-400 border border-emerald-800'
                            : 'bg-gray-800 text-gray-400'
                        }`}>
                          {c.status}
                        </span>

                        <button
                          onClick={() => handleSendToAgent(c.id)}
                          disabled={isSending === c.id}
                          className="bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white text-[10px] px-2 py-0.5 rounded font-medium transition-colors"
                        >
                          {isSending === c.id ? 'Sending...' : '⚡ Send to Agent'}
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>

              {/* Add Comment Form */}
              <form onSubmit={handleAddComment} className="p-3 border-t border-gray-800 bg-gray-900/60 flex flex-col space-y-2">
                <div className="flex space-x-2">
                  <input
                    type="number"
                    placeholder="Line #"
                    value={targetLine}
                    onChange={e => setTargetLine(e.target.value === '' ? '' : Number(e.target.value))}
                    className="w-20 bg-gray-950 border border-gray-800 rounded px-2 py-1 text-gray-200 placeholder-gray-600 text-[11px]"
                  />
                  <input
                    type="text"
                    placeholder="Snippet or code context (optional)"
                    value={selectedSnippet}
                    onChange={e => setSelectedSnippet(e.target.value)}
                    className="flex-1 bg-gray-950 border border-gray-800 rounded px-2 py-1 text-gray-200 placeholder-gray-600 text-[11px]"
                  />
                </div>
                <textarea
                  placeholder="Add code review note or change request..."
                  value={newCommentText}
                  onChange={e => setNewCommentText(e.target.value)}
                  rows={2}
                  className="w-full bg-gray-950 border border-gray-800 rounded p-2 text-gray-200 placeholder-gray-600 resize-none text-[11px]"
                />
                <button
                  type="submit"
                  disabled={!newCommentText.trim()}
                  className="w-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white font-medium py-1 rounded transition-colors text-[11px]"
                >
                  Post Review Comment
                </button>
              </form>
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-gray-500">
          Select an artifact to inspect and review.
        </div>
      )}
    </div>
  );
}
