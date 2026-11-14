import { updateTask } from '../../data/measurements'
import type { DailyTask } from '../../shared/types'

/** Bind an acknowledged write to the snapshot visible when its reply arrives. */
export async function confirmTaskUpdate(
  id: string,
  missionDate: string,
  completed: boolean,
  currentSnapshot: () => DailyTask,
  patch: (id: string, missionDate: string, completed: boolean) => Promise<DailyTask> = updateTask,
) {
  const value = await patch(id, missionDate, completed)
  return { snapshot: currentSnapshot(), value }
}
