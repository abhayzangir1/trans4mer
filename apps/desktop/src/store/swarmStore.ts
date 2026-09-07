import { create } from 'zustand';

export interface SwarmSelectionState {
  selectedAgentId: string | null;
  setSelectedAgent: (id: string | null) => void;
}

export const useSwarmStore = create<SwarmSelectionState>((set) => ({
  selectedAgentId: null,
  setSelectedAgent: (id) => set({ selectedAgentId: id }),
}));
