import { useEffect, useState, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Bot, User } from 'lucide-react';

interface EventEnvelope {
  sequence_id: number;
  event_id: string;
  event: any;
  actor_id: string;
  created_at: string;
}

interface Message {
  id: string;
  role: 'user' | 'agent';
  senderName: string;
  senderRole?: string;
  text: string;
  timestamp: string;
}

import { useUiStore } from '../../store/uiStore';
import { useConversationStore } from '../../store/conversationStore';
import { useAgents } from '../../hooks/useAgents';

export default function MessageList() {
  const { activeProjectId } = useUiStore();
  const { activeConversationId, activeDmAgent } = useConversationStore();
  const { agents } = useAgents(activeProjectId);
  const [messages, setMessages] = useState<Message[]>([]);
  const [streamingMessage, setStreamingMessage] = useState<{
    executionId: string;
    text: string;
    senderName: string;
    senderRole: string;
    timestamp: string;
  } | null>(null);
  const cursorRef = useRef<number>(0);
  const bottomRef = useRef<HTMLDivElement>(null);

  const targetChannelId = activeDmAgent 
    ? activeDmAgent.id 
    : (activeConversationId || 'general');

  const scrollToBottom = () => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, streamingMessage]);

  const resolveSenderInfo = (sender: any) => {
    const isUser = !!sender?.Human || (typeof sender === 'string' && sender.toLowerCase().includes('user'));
    
    if (isUser) {
      const displayName = sender?.Human?.display_name || 'You';
      return { isUser: true, senderName: displayName, senderRole: 'Human' };
    }

    const agentId = typeof sender?.Agent === 'string' ? sender.Agent : (typeof sender === 'string' ? sender : null);
    if (agentId) {
      const found = agents.find(a => a.id === agentId);
      if (found) {
        return {
          isUser: false,
          senderName: found.name || (found.definitionId ? found.definitionId.charAt(0).toUpperCase() + found.definitionId.slice(1) : 'Agent'),
          senderRole: found.role || 'Autonomous Specialist',
        };
      }
      return { isUser: false, senderName: 'Agent', senderRole: 'Autonomous Specialist' };
    }

    return { isUser: false, senderName: 'Agent', senderRole: 'Autonomous Specialist' };
  };

  const processEvent = (event: any, sequence_id: number) => {
    if (sequence_id > cursorRef.current) {
      cursorRef.current = sequence_id;
    }
    
    let eventType = event.type || event.event_type;
    let data = event.data || event;

    if (!eventType && typeof event === 'object') {
      const keys = Object.keys(event);
      if (keys.length === 1) {
        eventType = keys[0];
        data = event[keys[0]];
      }
    }

    if (eventType === 'TextDelta') {
      const execId = data.execution_id || data.executionId || 'exec-stream';
      const delta = data.delta || '';
      if (delta) {
        setStreamingMessage(prev => {
          if (prev && prev.executionId === execId) {
            return { ...prev, text: prev.text + delta };
          }
          return {
            executionId: execId,
            text: delta,
            senderName: activeDmAgent?.name || 'Agent',
            senderRole: activeDmAgent?.role || 'Autonomous Specialist',
            timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
          };
        });
      }
    }

    if (eventType === 'ExecutionCompleted' || eventType === 'ExecutionFailed') {
      setStreamingMessage(null);
    }

    if (eventType === 'MessageSent' || eventType === 'MessageCreated') {
      // Clear live streaming buffer as durable persisted message is now available
      setStreamingMessage(null);

      const msg = data.message || data;
      const msgId = msg.id || data.message_id || Date.now().toString();
      const content = msg.content || data.payload || '';
      
      // Filter message to current channel or DM
      const msgChannel = msg.channel_id || msg.channelId || 'general';
      if (activeDmAgent) {
        const isDmMatch = msgChannel === activeDmAgent.id || 
                          msgChannel === activeDmAgent.definition_id ||
                          msg.sender?.agent_instance_id === activeDmAgent.id;
        if (!isDmMatch) return;
      } else {
        if (msgChannel !== targetChannelId) return;
      }

      const { isUser, senderName, senderRole } = resolveSenderInfo(msg.sender);

      setMessages(prev => {
        if (prev.find(m => m.id === msgId)) return prev;
        return [...prev, {
          id: msgId,
          role: isUser ? 'user' : 'agent',
          senderName,
          senderRole,
          text: content,
          timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
        }];
      });
    }
  };

  useEffect(() => {
    let isMounted = true;
    setMessages([]);
    cursorRef.current = 0;

    const fetchState = async () => {
      try {
        const isRunningInTauri = typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ !== undefined);
        if (!isRunningInTauri) {
          setMessages([
            {
              id: 'msg-1',
              role: 'user',
              senderName: 'Abhay Zangir',
              senderRole: 'Human Operator',
              text: 'Assemble our core engineering swarm. Audit the Tokio semaphore concurrency scheduler and verify LanceDB hybrid vector retrieval.',
              timestamp: '10:42 AM',
            },
            {
              id: 'msg-2',
              role: 'agent',
              senderName: 'Boss Agent',
              senderRole: 'Chief AI Swarm Orchestrator',
              text: 'Decomposing objective into milestone sub-tasks: (1) Delegating scheduler audit to Rust Systems Engineer; (2) Delegating vector verification to Memory Specialist. Requesting Tokio permits...',
              timestamp: '10:42 AM',
            },
            {
              id: 'msg-3',
              role: 'agent',
              senderName: 'Rust Systems Engineer',
              senderRole: 'Tokio Concurrency & IPC Bridges',
              text: 'Permit acquired. Inspected `core/trans4mers-engine/src/scheduler.rs`: Project cap enforced at 4 concurrent tasks, node global concurrency semaphore capped at 8. Starvation-resistant age bonus active.',
              timestamp: '10:43 AM',
            },
            {
              id: 'msg-4',
              role: 'agent',
              senderName: 'Cognitive Memory Specialist',
              senderRole: '4-Tier RAG (LanceDB + FTS5 RRF)',
              text: 'Reciprocal Rank Fusion query completed across LanceDB 384-dim vector embeddings and SQLite FTS5 lexical index. 18 chunk records ranked with score cut-off 0.82. Memory pyramid stable.',
              timestamp: '10:43 AM',
            }
          ]);
          return;
        }
        // 1. Fetch genuine persisted message history from SQLite
        const history = await invoke<any[]>('get_messages', {
          projectId: activeProjectId,
          conversationId: activeDmAgent ? '' : (activeConversationId || activeProjectId),
          channelId: targetChannelId,
          limit: 100,
          offset: 0
        });

        if (!isMounted) return;

        if (history && Array.isArray(history)) {
          const mapped: Message[] = history.map((msg) => {
            const { isUser, senderName, senderRole } = resolveSenderInfo(msg.sender);
            return {
              id: msg.id,
              role: isUser ? 'user' : 'agent',
              senderName,
              senderRole,
              text: msg.content,
              timestamp: new Date(msg.created_at || Date.now()).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            };
          });
          
          setMessages((prev) => {
            const fetchedIds = new Set(mapped.map((m) => m.id));
            const liveOnly = prev.filter((m) => !fetchedIds.has(m.id));
            return [...mapped, ...liveOnly];
          });
        }

        if (!isMounted) return;
        // 2. Reconcile any un-replayed sequence events
        await invoke('replay_events', { projectId: activeProjectId, fromSequence: cursorRef.current });
      } catch (err) {
        if (isMounted) {
          console.error("Reconciliation failed:", err);
        }
      }
    };

    fetchState();

    const unlisten = listen<string>('domain_event', (event) => {
      if (!isMounted) return;
      try {
        const env: EventEnvelope = JSON.parse(event.payload);
        processEvent(env.event, env.sequence_id);
      } catch (err) {
        console.error("Failed to parse event", err);
      }
    });

    const onFocus = () => {
      if (isMounted) fetchState();
    };
    window.addEventListener('focus', onFocus);

    return () => {
      isMounted = false;
      unlisten.then(f => f());
      window.removeEventListener('focus', onFocus);
    };
  }, [activeProjectId, targetChannelId, activeConversationId, activeDmAgent?.id]);

  const getRoleBadgeStyle = (role?: string) => {
    if (!role) return 'bg-gray-800 text-gray-400 border border-gray-700';
    const lower = role.toLowerCase();
    if (lower.includes('lead') || lower.includes('boss')) return 'bg-amber-500/15 text-amber-400 border border-amber-500/30';
    if (lower.includes('code') || lower.includes('engineer') || lower.includes('dev')) return 'bg-blue-500/15 text-blue-400 border border-blue-500/30';
    if (lower.includes('ui') || lower.includes('frontend') || lower.includes('design')) return 'bg-purple-500/15 text-purple-400 border border-purple-500/30';
    if (lower.includes('architect') || lower.includes('planner') || lower.includes('strategy')) return 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30';
    if (lower.includes('audit') || lower.includes('reviewer') || lower.includes('security') || lower.includes('guard')) return 'bg-rose-500/15 text-rose-400 border border-rose-500/30';
    if (lower.includes('research') || lower.includes('analyst') || lower.includes('data')) return 'bg-cyan-500/15 text-cyan-400 border border-cyan-500/30';
    if (lower.includes('test') || lower.includes('qa') || lower.includes('quality')) return 'bg-teal-500/15 text-teal-400 border border-teal-500/30';
    if (lower.includes('human') || lower.includes('user')) return 'bg-brand-primary/20 text-brand-primary border border-brand-primary/40';

    // Deterministic palette mapping for any custom specialist role
    const PALETTES = [
      'bg-indigo-500/15 text-indigo-400 border border-indigo-500/30',
      'bg-violet-500/15 text-violet-400 border border-violet-500/30',
      'bg-orange-500/15 text-orange-400 border border-orange-500/30',
      'bg-lime-500/15 text-lime-400 border border-lime-500/30',
      'bg-fuchsia-500/15 text-fuchsia-400 border border-fuchsia-500/30',
      'bg-sky-500/15 text-sky-400 border border-sky-500/30',
    ];
    let hash = 0;
    for (let i = 0; i < lower.length; i++) hash = (hash * 31 + lower.charCodeAt(i)) >>> 0;
    return PALETTES[hash % PALETTES.length];
  };

  const renderFormattedContent = (text: string, isStreaming = false) => {
    const trimmed = text.trim();
    if (
      trimmed.startsWith('{') &&
      (trimmed.includes('"tool"') ||
        trimmed.includes('"arguments"') ||
        trimmed.includes('"tool_name"') ||
        trimmed.includes('"name"'))
    ) {
      try {
        const parsed = JSON.parse(trimmed);
        const toolName = parsed.tool || parsed.tool_name || parsed.name;
        if (toolName) {
          const args = parsed.arguments || parsed.parameters;
          const argSnippet = args ? JSON.stringify(args) : '';
          return (
            <div className="flex flex-col space-y-1 my-1 p-2 rounded bg-black/40 border border-brand-primary/30 text-xs font-mono">
              <div className="flex items-center space-x-2 text-brand-primary font-semibold">
                <span className="w-2 h-2 rounded-full bg-brand-primary animate-pulse inline-block" />
                <span>Invoking Tool: <code className="text-white bg-gray-800 px-1.5 py-0.5 rounded">{toolName}</code></span>
              </div>
              {argSnippet && argSnippet !== '{}' && (
                <div className="text-[11px] text-gray-400 truncate max-w-lg">
                  Parameters: <span className="text-gray-300">{argSnippet}</span>
                </div>
              )}
            </div>
          );
        }
      } catch {
        const match = trimmed.match(/"(?:tool|tool_name|name)"\s*:\s*"([^"]+)"/);
        if (match) {
          return (
            <div className="flex items-center space-x-2 my-1 p-2 rounded bg-black/40 border border-brand-primary/30 text-xs font-mono text-brand-primary">
              <span className="w-2 h-2 rounded-full bg-brand-primary animate-pulse inline-block" />
              <span>Invoking Tool: <code className="text-white bg-gray-800 px-1.5 py-0.5 rounded">{match[1]}</code></span>
            </div>
          );
        }
      }
    }

    let cleanedText = text;
    if (cleanedText.startsWith('Thought: ')) {
      cleanedText = cleanedText.substring(9);
    }
    cleanedText = cleanedText.replace(/\s*Action:\s*(?:None|Finished|null)\s*$/i, '').trim();

    return (
      <div className={`text-gray-200 text-xs leading-relaxed whitespace-pre-wrap break-words selection:bg-brand-primary selection:text-black ${isStreaming ? 'font-mono' : ''}`}>
        {cleanedText || text}
        {isStreaming && <span className="inline-block w-1.5 h-3 bg-emerald-400 ml-0.5 animate-pulse" />}
      </div>
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-4 font-mono">
      {messages.length === 0 && !streamingMessage && (
        <div className="flex flex-col items-center justify-center h-full text-center text-gray-500 py-12">
          <Bot size={32} className="mb-2 opacity-20" />
          <p className="text-xs font-semibold">Welcome to this sovereign channel</p>
          <p className="text-[10px] mt-1 text-gray-600 max-w-xs">
            Messages sent here are automatically routed to the lead orchestrator or specialists mentioned with @.
          </p>
        </div>
      )}

      {messages.map((msg) => {
        const isUser = msg.role === 'user';

        return (
          <div key={msg.id} className="flex items-start space-x-3 group hover:bg-white/[0.02] p-1.5 -mx-1.5 rounded-lg transition-colors">
            {/* Avatar */}
            <div className={`w-8 h-8 rounded-lg flex items-center justify-center shrink-0 ${
              isUser ? 'bg-brand-primary text-black font-bold' : 'bg-gray-800 text-gray-300 border border-gray-700'
            }`}>
              {isUser ? <User size={15} /> : <Bot size={15} />}
            </div>

            {/* Message Body */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center space-x-2 mb-1">
                <span className={`font-bold ${isUser ? 'text-gray-100' : 'text-brand-primary'}`}>
                  {msg.senderName}
                </span>
                {msg.senderRole && (
                  <span className={`text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase tracking-wider ${getRoleBadgeStyle(msg.senderRole)}`}>
                    {msg.senderRole}
                  </span>
                )}
                <span className="text-[10px] text-gray-500">{msg.timestamp}</span>
              </div>

              {renderFormattedContent(msg.text)}
            </div>
          </div>
        );
      })}

      {streamingMessage && (
        <div key="streaming-active" className="flex items-start space-x-3 group hover:bg-white/[0.02] p-1.5 -mx-1.5 rounded-lg transition-colors border-l-2 border-emerald-500 pl-2 bg-emerald-500/5">
          {/* Avatar */}
          <div className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-gray-800 text-emerald-400 border border-emerald-500/40">
            <Bot size={15} className="animate-pulse" />
          </div>

          {/* Message Body */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center space-x-2 mb-1">
              <span className="font-bold text-emerald-400">
                {streamingMessage.senderName}
              </span>
              <span className="text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase tracking-wider bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 flex items-center gap-1">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-ping inline-block" />
                Live Streaming
              </span>
              <span className="text-[10px] text-gray-500">{streamingMessage.timestamp}</span>
            </div>

            {renderFormattedContent(streamingMessage.text, true)}
          </div>
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  );
}
