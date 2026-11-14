import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import { TaskPanel } from './TaskPanel'

it('shows server-confirmed completion for the displayed UTC mission date', () => {
  const html = renderToStaticMarkup(<TaskPanel connected onChanged={() => {}} tasks={{ missionDate: '2026-11-14', tasks: [
    { id: 'a', title: 'Заполнить измерения', completedAt: null },
    { id: 'b', title: 'Проверить запись', completedAt: '2026-11-14T08:30:00Z' },
  ] }} />)
  expect(html).toContain('1 / 2')
  expect(html.match(/checked=""/g)).toHaveLength(1)
  expect(html).toContain('Ожидает выполнения')
  expect(html).toContain('Выполнено')
  expect(html).toContain('Дата миссии (UTC): 2026-11-14')
  expect(html).toContain('Новый день имеет отдельный список выполнения')
})

it('keeps an offline snapshot visible while disabling unconfirmed changes', () => {
  const html = renderToStaticMarkup(<TaskPanel connected={false} onChanged={() => {}} tasks={{ missionDate: '2026-11-14', tasks: [
    { id: 'a', title: 'Заполнить измерения', completedAt: null },
  ] }} />)
  expect(html).toContain('disabled=""')
  expect(html).toContain('Заполнить измерения')
  expect(html).toContain('Показаны последние полученные отметки')
})

it('distinguishes a missing response from an empty task list', () => {
  const offline = renderToStaticMarkup(<TaskPanel connected={false} onChanged={() => {}} />)
  const empty = renderToStaticMarkup(<TaskPanel connected onChanged={() => {}} tasks={{ missionDate: '2026-11-14', tasks: [] }} />)
  expect(offline).toContain('Подключитесь к серверу')
  expect(offline).not.toContain('Задач пока нет')
  expect(empty).toContain('Задач пока нет')
})
