import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Conversation {
  id: string;
  project_id: string;
  title: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface TeamMember {
  id: string;
  name: string;
  role: string;
  definition_id: string;
  status: string;
}

interface ConversationState {
  conversations: Conversation[];
  activeConversationId: string | null;
  activeDmAgent: TeamMember | null;
  sidebarMode: 'team' | 'files';
  setSidebarMode: (mode: 'team' | 'files') => void;
  fetchConversations: (projectId: string) => Promise<void>;
  createConversation: (projectId: string, title: string) => Promise<Conversation | null>;
  clearConversations: () => void;
  setActiveConversation: (id: string | null) => void;
  setActiveDmAgent: (agent: TeamMember | null) => void;
}

export const useConversationStore = create<ConversationState>((set, get) => ({
  conversations: [],
  activeConversationId: null,
  activeDmAgent: null,
  sidebarMode: 'team',
  setSidebarMode: (mode) => set({ sidebarMode: mode }),
  clearConversations: () => set({ conversations: [], activeConversationId: null, activeDmAgent: null }),
  fetchConversations: async (projectId: string) => {
    try {
      const isRunningInTauri = typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ !== undefined);
      if (!isRunningInTauri) {
        set({ conversations: [], activeConversationId: null, activeDmAgent: null });
        return;
      }
      const convs = await invoke<Conversation[]>('list_conversations', { projectId });
      set({ conversations: convs });
      
      const current = get().activeConversationId;
      const isCurrentInConvs = convs.some(c => c.id === current);
      if ((!current || !isCurrentInConvs) && convs.length > 0) {
        const general = convs.find(c => c.title.toLowerCase() === 'general') || convs[0];
        set({ activeConversationId: general.id, activeDmAgent: null });
      } else if (convs.length === 0) {
        set({ activeConversationId: null, activeDmAgent: null });
      }
    } catch (e) {
      console.error('Failed to fetch conversations', e);
    }
  },
  createConversation: async (projectId: string, title: string) => {
    try {
      const conv = await invoke<Conversation>('create_conversation', { projectId, title });
      set((state) => ({
        conversations: [...state.conversations, conv],
        activeConversationId: conv.id,
        activeDmAgent: null,
      }));
      return conv;
    } catch (e) {
      console.error('Failed to create conversation', e);
      return null;
    }
  },
  setActiveConversation: (id) => set({ activeConversationId: id, activeDmAgent: null }),
  setActiveDmAgent: (agent) => set({ activeDmAgent: agent, activeConversationId: null }),
}));
