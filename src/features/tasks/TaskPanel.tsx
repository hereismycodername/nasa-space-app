import { useLayoutEffect, useRef, useState } from 'react'
import type { DailyTask } from '../../shared/types'
import { confirmTaskUpdate } from './confirmation'
import '../check-in/forms.css'

function TaskRow({ task, connected, onChanged }: { task: DailyTask; connected: boolean; onChanged: () => void }) {
  const [pending, setPending] = useState(false)
  const [confirmed, setConfirmed] = useState<{ snapshot: DailyTask; value: DailyTask }>()
  const [error, setError] = useState('')
  const latestTask = useRef(task)
  useLayoutEffect(() => { latestTask.current = task }, [task])
  // Show an acknowledged PATCH until the parent supplies its next server snapshot.
  const displayed = confirmed?.snapshot === task ? confirmed.value : task

  async function change(completed: boolean) {
    if (pending) return
    setPending(true)
    setError('')
    try {
      setConfirmed(await confirmTaskUpdate(task.id, completed, () => latestTask.current))
      onChanged()
    } catch {
      setError('Не удалось подтвердить изменение. Обновите данные и повторите действие.')
      onChanged()
    } finally {
      setPending(false)
    }
  }

  return <li className="task-row" aria-busy={pending}>
    <label className="task-choice">
      <input type="checkbox" checked={displayed.completedAt !== null} disabled={!connected || pending}
        onChange={event => { void change(event.target.checked) }} />
      <span><span className={displayed.completedAt ? 'task-title task-title-complete' : 'task-title'}>{task.title}</span>
        <span className="task-detail">{pending ? 'Сохраняем отметку…' : displayed.completedAt
          ? `Выполнено ${new Date(displayed.completedAt).toLocaleString('ru-RU')}` : 'Ожидает выполнения'}</span>
      </span>
    </label>
    {error ? <p className="form-error" role="alert">{error}</p> : null}
  </li>
}

export function TaskPanel({ tasks, connected, onChanged }: { tasks?: DailyTask[]; connected: boolean; onChanged: () => void }) {
  const completed = tasks?.filter(task => task.completedAt !== null).length ?? 0
  return <section className="panel task-panel" aria-labelledby="task-title">
    <div className="task-heading"><h2 id="task-title">Задачи экипажа</h2>
      {tasks ? <span className="task-counter">{completed} / {tasks.length}</span> : null}
    </div>
    {!tasks ? <p className="form-hint">{connected ? 'Загрузка задач…' : 'Подключитесь к серверу, чтобы получить задачи.'}</p>
      : tasks.length === 0 ? <p className="form-hint">Задач пока нет.</p>
        : <ul className="task-list">{tasks.map(task => <TaskRow key={task.id} task={task} connected={connected} onChanged={onChanged} />)}</ul>}
    {tasks && !connected ? <p className="form-hint">Показаны последние полученные отметки. Изменение доступно после восстановления связи.</p> : null}
    <p className="task-footnote">Отметки сохраняются до следующего изменения; автоматического сброса по дням пока нет.</p>
  </section>
}
