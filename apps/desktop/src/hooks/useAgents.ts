import { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export interface AgentNode {
  id: string;
  projectId: string;
  status: string;
  definitionId: string;
  name?: string;
  role?: string;
  parentId?: string;
}

export function useAgents(projectId: string | null) {
  const [agents, setAgents] = useState<AgentNode[]>([]);
  const cursorRef = useRef<number>(0);

  useEffect(() => {
    if (!projectId) return;

    let isMounted = true;
    setAgents([]);
    cursorRef.current = 0;

    const processEvent = (event: any, sequence_id: number) => {
      if (sequence_id > cursorRef.current) cursorRef.current = sequence_id;
      let eventType = event.type || event.event_type;
      let data = event.data || event;

      if (!eventType && typeof event === 'object') {
        const keys = Object.keys(event);
        if (keys.length === 1) {
          eventType = keys[0];
          data = event[keys[0]];
        }
      }

      if (data.project_id && data.project_id !== projectId) return;

      if (eventType === 'AgentSpawned' || eventType === 'AgentInstanceCreated') {
        setAgents((prev) => {
          const agentId = data.agent_id || data.agent_instance_id;
          if (prev.find(a => a.id === agentId)) return prev;
          return [...prev, {
            id: agentId,
            projectId: data.project_id || projectId,
            status: 'Idle',
            definitionId: data.definition_id,
            name: data.name,
            role: data.role,
            parentId: data.parent_id || undefined,
          }];
        });
      } else if (eventType === 'AgentStatusChanged') {
        setAgents((prev) => prev.map(a => 
          a.id === (data.agent_instance_id || data.agent_id) ? { ...a, status: data.new_status } : a
        ));
      }
    };

    const fetchState = async () => {
      try {
        const isRunningInTauri = typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ !== undefined);
        if (!isRunningInTauri) {
          setAgents([
            {
              id: 'agent-boss',
              projectId: projectId,
              status: 'Running',
              definitionId: 'orchestrator',
              name: 'Boss Agent',
              role: 'Chief AI Swarm Orchestrator',
            },
            {
              id: 'agent-rust-systems',
              projectId: projectId,
              status: 'Working',
              definitionId: 'rust_engineer',
              name: 'Rust Systems Engineer',
              role: 'Tokio Concurrency & IPC Bridges',
              parentId: 'agent-boss',
            },
            {
              id: 'agent-security',
              projectId: projectId,
              status: 'Idle',
              definitionId: 'security_auditor',
              name: 'Security & Policy Auditor',
              role: 'Zero-Trust LCS Diff Reviewer',
              parentId: 'agent-boss',
            },
            {
              id: 'agent-memory',
              projectId: projectId,
              status: 'Idle',
              definitionId: 'memory_specialist',
              name: 'Cognitive Memory Specialist',
              role: '4-Tier RAG (LanceDB + FTS5 RRF)',
              parentId: 'agent-boss',
            },
            {
              id: 'agent-cdp',
              projectId: projectId,
              status: 'Idle',
              definitionId: 'browser_navigator',
              name: 'CDP Browser Navigator',
              role: 'Headless DOM & Live Mirror',
              parentId: 'agent-boss',
            }
          ]);
          return;
        }
        const initial = await invoke<any[]>('list_agents', { projectId });
        if (isMounted && initial && Array.isArray(initial)) {
          const mapped: AgentNode[] = initial.map((a: any) => ({
            id: a.id,
            projectId: a.project_id || projectId,
            status: a.status || 'Idle',
            definitionId: a.definition_id,
            name: a.name,
            role: a.role,
            parentId: a.parent_instance_id || undefined,
          }));
          setAgents((prev) => {
            const fetchedIds = new Set(mapped.map((a) => a.id));
            const liveOnly = prev.filter((a) => !fetchedIds.has(a.id) && a.projectId === projectId);
            return [...mapped, ...liveOnly];
          });
        }
        await invoke('replay_events', { projectId, fromSequence: cursorRef.current });
      } catch (err) {
        console.error("Agent reconciliation failed:", err);
      }
    };

    fetchState();

    const unlisten = listen<string>('domain_event', (event) => {
      if (!isMounted) return;
      try {
        const env = JSON.parse(event.payload);
        processEvent(env.event, env.sequence_id);
      } catch (err) {
        console.error("Failed to parse agent event", err);
      }
    });

    const onFocus = () => fetchState();
    window.addEventListener('focus', onFocus);

    return () => {
      isMounted = false;
      unlisten.then(f => f());
      window.removeEventListener('focus', onFocus);
    };
  }, [projectId]);

  return { agents };
}
