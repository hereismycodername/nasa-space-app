import { updateTask } from '../../data/measurements'
import type { DailyTask } from '../../shared/types'

/** Bind an acknowledged write to the snapshot visible when its reply arrives. */
export async function confirmTaskUpdate(
  id: string,
  completed: boolean,
  currentSnapshot: () => DailyTask,
  patch: (id: string, completed: boolean) => Promise<DailyTask> = updateTask,
) {
  const value = await patch(id, completed)
  return { snapshot: currentSnapshot(), value }
}
