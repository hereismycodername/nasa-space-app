import { expect, it, vi } from 'vitest'
import type { DailyTask } from '../../shared/types'
import { confirmTaskUpdate } from './confirmation'

it('keeps a successful PATCH visible when polling replaced the task while the write was pending', async () => {
  const original: DailyTask = { id: 'a', title: 'Заполнить измерения', completedAt: null }
  let current = original
  let respond!: (task: DailyTask) => void
  const patch = vi.fn(() => new Promise<DailyTask>(resolve => { respond = resolve }))
  const acknowledgement = confirmTaskUpdate('a', true, () => current, patch)

  // A concurrent GET completes before PATCH; its snapshot still lacks the new mark.
  const polled = { ...original }
  current = polled
  const saved = { ...original, completedAt: '2026-11-14T08:30:00Z' }
  respond(saved)

  const confirmed = await acknowledgement
  expect(confirmed.snapshot).toBe(polled)
  expect(confirmed.snapshot).not.toBe(original)
  expect(confirmed.value).toBe(saved)
  expect(patch).toHaveBeenCalledExactlyOnceWith('a', true)
})

it('does not acknowledge a failed task update or read a new snapshot as if it were saved', async () => {
  const currentSnapshot = vi.fn<() => DailyTask>()
  const patch = vi.fn<() => Promise<DailyTask>>().mockRejectedValue(new TypeError('Connection lost'))
  await expect(confirmTaskUpdate('a', false, currentSnapshot, patch)).rejects.toThrow('Connection lost')
  expect(currentSnapshot).not.toHaveBeenCalled()
})
