/* AstroCheck pitch deck engine: stage scaling, navigation, pitch timer, speaker notes, and the pieces that
   are built from data (anatomy figure, pulse chart, terminal frame). Slides fire `deck:enter` / `deck:leave`
   so the motion layer can hook in without touching this file. */
(() => {
  'use strict'
  const $ = (s, r = document) => r.querySelector(s)
  const $$ = (s, r = document) => [...r.querySelectorAll(s)]
  const el = (tag, cls, html) => { const e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e }
  const NS = 'http://www.w3.org/2000/svg'
  const svg = (tag, attrs = {}, text) => { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); if (text != null) e.textContent = text; return e }
  const mmss = s => `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(Math.floor(s % 60)).padStart(2, '0')}`

  // ------------------------------------------------------------------ stage: 1920 x 1080, scaled to the window
  const stage = $('#stage')
  const fit = () => { stage.style.transform = `translate(-50%, -50%) scale(${Math.min(innerWidth / 1920, innerHeight / 1080)})` }
  addEventListener('resize', fit)
  fit()

  // ------------------------------------------------------------------ anatomy figure
  const CALLOUTS = [ // anchors and label rows from ui/crates/ui-model/src/anatomy.rs (design units)
    { sys: 'brain', label: 'Мозг · сон', side: 'right', ax: 119.8, ay: 29.3, ly: 24 },
    { sys: 'eyes', label: 'Зрение', side: 'left', ax: 110.8, ay: 42.1, ly: 43 },
    { sys: 'lungs', label: 'Дыхание', side: 'left', ax: 101.0, ay: 126.8, ly: 122 },
    { sys: 'immune', label: 'Температура', side: 'right', ax: 119.5, ay: 108.9, ly: 100 },
    { sys: 'heart', label: 'Сердце', side: 'right', ax: 129.7, ay: 130.0, ly: 136 },
    { sys: 'gut', label: 'Обмен веществ', side: 'right', ax: 129.5, ay: 167.7, ly: 178 },
    { sys: 'msk', label: 'Кости и мышцы', side: 'right', ax: 146.2, ay: 314.3, ly: 318 },
  ]
  const STATUS_TEXT = { warn: 'Внимание', crit: 'Срочно' }

  function buildFigure(panel) {
    const A = window.ANATOMY
    if (!A) return
    const H = +panel.dataset.h || 800
    const top = +panel.dataset.top || 60
    const fs = H / A.h
    const W = A.w * fs
    const PW = panel.clientWidth, PH = panel.clientHeight
    const left = Math.round((PW - W) / 2)
    const status = JSON.parse(panel.dataset.status || '{}')

    const box = el('div', 'figbox')
    box.style.cssText = `left:${left}px;top:${top}px;width:${W}px;height:${H}px`
    const fig = el('div', 'fig')
    fig.style.setProperty('--fs', fs)
    fig.dataset.mode = panel.dataset.mode || 'organs'
    const img = (src, b, cls) => {
      const i = new Image()
      i.src = 'assets/anatomy/' + src
      i.draggable = false
      i.alt = ''
      if (cls) i.className = cls
      i.style.cssText = `left:${b.x}px;top:${b.y}px;width:${b.w}px;height:${b.h}px`
      return i
    }
    const layer = (cls, ...kids) => { const l = el('div', 'lay ' + cls); kids.forEach(k => l.append(k)); fig.append(l); return l }
    layer('l-body', img('body.webp', A.skin))
    layer('l-skel', img('skeleton.webp', A.skeleton))
    layer('l-org', ...A.organs.map(o => { const i = img(o.src, o, 'org'); i.dataset.sys = o.sys; i.dataset.name = o.name; i.dataset.pulse = o.pulse; return i }))
    layer('l-xorg', img('organs-xray.webp', A.organsXray))
    layer('l-xskel', img('skeleton-xray.webp', A.skeleton))
    layer('l-glass', img('glass.webp', A.skin))
    const out = layer('l-out')
    for (const sys in A.outlines) {
      for (const kind of ['warn', 'crit', 'white']) {
        const i = img(`outline-${sys}-${kind}.png`, A.outlines[sys], `out out-${kind}`)
        i.dataset.sys = sys
        if (status[sys] === kind) i.classList.add('on')
        out.append(i)
      }
    }
    box.append(fig)
    panel.append(box)

    // measurement marks, leader lines and labels are drawn in panel pixels (not scaled with the sprites)
    const u = fs * A.tpu // design unit -> px
    const px = (x, y) => [left + x * u, top + y * u]
    const marks = svg('svg', { class: 'figmarks', viewBox: `0 0 ${PW} ${PH}` })
    const yTop = top + A.skin.y * fs, yBottom = top + (A.skin.y + A.skin.h) * fs
    const cm = (yBottom - yTop) / 174
    for (let k = 0; k * 10 <= 174; k++) {
      const y = yBottom - k * 10 * cm
      marks.append(svg('line', { class: 'tick', x1: 1, x2: k % 5 === 0 ? 10 : 6, y1: y, y2: y }))
    }
    marks.append(svg('text', { x: 18, y: PH - 16 }, 'R'), svg('text', { x: PW - 18, y: PH - 16, 'text-anchor': 'end' }, 'L'),
      svg('text', { x: PW / 2, y: PH - 16, 'text-anchor': 'middle', opacity: .7 }, 'ANTERIOR'))
    panel.append(marks)

    const LW = 150, PAD = 22
    for (const c of CALLOUTS) {
      const [ax, ay] = px(c.ax, c.ay)
      const ly = top + c.ly * u
      const dir = c.side === 'left' ? -1 : 1
      const edge = c.side === 'left' ? PAD + LW + 6 : PW - PAD - LW - 6
      const run = Math.abs(edge - ax)
      const diag = Math.min(Math.abs(ly - ay), Math.max(run - 10, 0))
      const ex = ax + dir * diag
      const len = Math.hypot(ex - ax, ly - ay)
      const sx = len > 8 ? ax + (ex - ax) / len * 8 : ax + dir * 8
      const sy = len > 8 ? ay + (ly - ay) / len * 8 : ay
      const st = status[c.sys]
      const g = svg('g', { class: 'co', 'data-sys': c.sys })
      g.append(svg('path', { class: 'lead', pathLength: 1, d: `M${sx} ${sy}${len > 8 ? ` L${ex} ${ly}` : ''} L${edge} ${ly}` }),
        svg('line', { class: 'lead', x1: edge, x2: edge, y1: ly - 4, y2: ly + 4 }))
      marks.append(g)
      const m = el('i', 'marker' + (st ? ' ' + st : ''))
      m.dataset.sys = c.sys
      m.style.cssText = `left:${ax}px;top:${ay}px`
      const lab = el('div', `callout ${c.side}${st ? ' ' + st : ''}`, `${c.label}<small>${STATUS_TEXT[st] || 'Без срабатываний'}</small>`)
      lab.dataset.sys = c.sys
      lab.style.cssText = c.side === 'left' ? `left:${PAD}px;top:${ly}px;width:${LW}px` : `right:${PAD}px;top:${ly}px;width:${LW}px`
      panel.append(m, lab)
    }
    panel.figure = { fig, setMode: mode => { fig.dataset.mode = mode; $$('.layer-tabs span', panel).forEach(t => t.classList.toggle('on', t.dataset.mode === mode)) } }
  }

  // ------------------------------------------------------------------ pulse chart: 7 days, hourly mean, one real gap
  function buildPulseChart(host) {
    if (!host) return
    const X0 = 64, X1 = 1112, Y0 = 18, Y1 = 438, LO = 55, HI = 105, HOURS = 168
    const x = h => X0 + (X1 - X0) * h / (HOURS - 1)
    const y = v => Y1 - (Y1 - Y0) * (v - LO) / (HI - LO)
    const noise = i => { const s = Math.sin(i * 127.1 + 311.7) * 43758.5453; return (s - Math.floor(s)) * 2 - 1 }
    const data = []
    for (let h = 0; h < HOURS; h++) {
      const hod = (h + 6) % 24
      const peak = Math.max(0, 1 - Math.abs(hod - 11)) * 13 + Math.max(0, 1 - Math.abs(hod - 16)) * 15
      let v = 67.5 + 5 * Math.cos((hod - 15) * Math.PI / 12) + peak + noise(h) * 1.6
      if (h >= 96) v += 13 + noise(h + 500) * 1.2
      data.push(h >= 52 && h <= 63 ? null : v)
    }
    for (const v of [60, 70, 80, 90, 100]) {
      host.append(svg('line', { class: 'gridl', x1: X0, x2: X1, y1: y(v), y2: y(v) }), svg('text', { class: 'axis', x: X0 - 14, y: y(v) + 5, 'text-anchor': 'end' }, v))
    }
    for (let d = 0; d <= 7; d++) {
      const h = Math.min(d * 24, HOURS - 1)
      host.append(svg('line', { class: 'gridl', x1: x(h), x2: x(h), y1: Y1, y2: Y1 + 8 }),
        svg('text', { class: 'axis', x: x(h), y: Y1 + 30, 'text-anchor': d === 0 ? 'start' : d === 7 ? 'end' : 'middle' }, `${String(8 + d).padStart(2, '0')}.11`))
    }
    // the last three days run higher: shown as a zone, not smoothed away
    const zone = svg('g', { class: 'g-zone' })
    zone.append(svg('rect', { class: 'zone', x: x(96), y: Y0, width: X1 - x(96), height: Y1 - Y0 }),
      svg('line', { class: 'zone-l', x1: x(96), x2: x(96), y1: Y0, y2: Y1 }),
      svg('text', { class: 'note', x: x(96) + 14, y: Y0 + 24, fill: '#f2b632' }, '+12–15 УД/МИН · ПОСЛЕДНИЕ 3 СУТОК'))
    host.append(zone)
    const gap = svg('g', { class: 'g-gap' })
    gap.append(svg('rect', { class: 'gap', x: x(51.4), y: Y0, width: x(63.6) - x(51.4), height: Y1 - Y0 }),
      svg('text', { class: 'note', x: (x(51.4) + x(63.6)) / 2, y: Y0 + 26, 'text-anchor': 'middle', fill: '#4fb0d6' }, 'ПРОПУСК'),
      svg('text', { class: 'note', x: (x(51.4) + x(63.6)) / 2, y: Y0 + 46, 'text-anchor': 'middle', fill: '#4fb0d6' }, 'НЕТ ДАННЫХ'))
    host.append(gap)
    let run = []
    const flush = () => { if (run.length) host.append(svg('path', { class: 'line', pathLength: 1, d: 'M' + run.map(p => `${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join(' L') })); run = [] }
    data.forEach((v, h) => { if (v == null) flush(); else run.push([x(h), y(v)]) })
    flush()
    const dots = svg('g', { class: 'g-dots' })
    data.forEach((v, h) => { if (v != null) dots.append(svg('circle', { class: 'dot', cx: x(h).toFixed(1), cy: y(v).toFixed(1), r: 2.6 })) })
    host.append(dots)
  }

  // ------------------------------------------------------------------ terminal client frame (braille dots on a canvas)
  function drawTerminal(canvas) {
    const rows = window.TERM_FIGURE
    if (!canvas || !rows) return
    const c = canvas.getContext('2d')
    const W = canvas.width, H = canvas.height
    c.fillStyle = '#06080a'
    c.fillRect(0, 0, W, H)
    c.font = '500 15px "DM Mono", monospace'
    c.textBaseline = 'middle'
    c.fillStyle = '#e6e9ed'
    c.fillText('ASTROCHECK', 14, 18)
    c.fillStyle = '#858e9b'
    c.fillText('CREW  Day 15 | LINK OK  RTT 1 s', 124, 18)
    c.strokeStyle = '#3a4350'
    c.strokeRect(12.5, 34.5, 150, 248)
    c.strokeRect(172.5, 34.5, W - 186, 248)
    c.fillStyle = '#858e9b'
    c.font = '400 12px "DM Mono", monospace'
    c.fillText(' BODY ', 22, 35)
    c.fillText(' INDICATORS ', 182, 35)
    const pitch = 1.95, ox = 28, oy = 44
    c.fillStyle = '#9db9e8'
    const BITS = [[0, 0, 1], [0, 1, 2], [0, 2, 4], [1, 0, 8], [1, 1, 16], [1, 2, 32], [0, 3, 64], [1, 3, 128]]
    rows.forEach((row, r) => [...row].forEach((ch, col) => {
      const bits = ch.codePointAt(0) - 0x2800
      if (bits <= 0 || bits > 255) return
      for (const [dx, dy, bit] of BITS) if (bits & bit) c.fillRect(ox + (col * 2 + dx) * pitch, oy + (r * 4 + dy) * pitch, 1.5, 1.5)
    }))
    const lines = [
      ['Resting heart rate', '85 bpm', 'WATCH', '#f2b632'], ['Blood pressure sys', '129 mmHg', 'WATCH', '#f2b632'],
      ['Body temperature', '36.6 C', 'NORMAL', '#9aa5b4'], ['Sleep last night', '4.1 h', 'WATCH', '#f2b632'],
      ['SpO2', '98 %', 'NORMAL', '#9aa5b4'], ['Mood', '2 /5', 'WATCH', '#f2b632'],
    ]
    c.font = '400 13px "DM Mono", monospace'
    lines.forEach(([name, value, st, col], i) => {
      const yy = 62 + i * 26
      c.fillStyle = '#c0c8d3'
      c.fillText(name, 184, yy)
      c.fillStyle = '#e6e9ed'
      c.fillText(value, 352, yy)
      c.fillStyle = col
      c.fillRect(440, yy - 4, 7, 7)
      c.fillText(st, 454, yy)
    })
    c.fillStyle = '#858e9b'
    c.fillText('Up/Dn move  Enter log  r ground  q quit', 184, 262)
  }

  // ------------------------------------------------------------------ navigation, timer, notes
  const slides = $$('.slide')
  const total = slides.length
  const durs = slides.map(s => +s.dataset.dur || 20)
  const ends = durs.map((_, i) => durs.slice(0, i + 1).reduce((a, b) => a + b, 0))
  const progress = $('#sb-progress')
  slides.forEach(() => progress.append(el('i')))
  let cur = -1
  let elapsed = 0, running = false, auto = false, last = performance.now()

  function go(i, opts = {}) {
    i = Math.max(0, Math.min(total - 1, i))
    if (i === cur) return
    const prev = slides[cur]
    if (prev) { prev.classList.remove('is-active'); prev.dispatchEvent(new CustomEvent('deck:leave', { bubbles: true })) }
    const dir = i > cur ? 1 : -1
    cur = i
    const s = slides[cur]
    s.classList.add('is-active')
    s.dispatchEvent(new CustomEvent('deck:enter', { bubbles: true, detail: { index: cur, dir } }))
    $('#sb-count').textContent = `${String(cur + 1).padStart(2, '0')}/${String(total).padStart(2, '0')}`
    $('#sb-title').textContent = (s.dataset.title || '').toUpperCase()
    $('#sb-budget').textContent = `ПЛАН ${durs[cur]} С · ДО ${mmss(ends[cur])}`
    $$('i', progress).forEach((seg, k) => { seg.className = k < cur ? 'done' : k === cur ? 'on' : '' })
    $$('#sections span').forEach(t => t.classList.toggle('on', t.dataset.sec === s.dataset.sec))
    $('#notes-title').textContent = `ЗАМЕТКИ · ${String(cur + 1).padStart(2, '0')} ${(s.dataset.title || '').toUpperCase()}`
    $('#notes-time').textContent = `${durs[cur]} С · ДО ${mmss(ends[cur])}`
    $('#notes-text').textContent = $('aside.notes', s)?.textContent ?? ''
    if (!opts.silent) history.replaceState(null, '', `#${cur + 1}`)
    if (cur > 0 && !running && elapsed === 0 && !opts.silent) running = true // the clock starts with the first advance
  }
  const next = () => go(cur + 1)
  const prev = () => go(cur - 1)

  function tick(now) {
    const dt = now - last
    last = now
    if (running) elapsed += dt
    const sec = elapsed / 1000
    $('#clock-now').textContent = mmss(sec)
    const clock = $('#clock')
    clock.classList.toggle('running', running)
    clock.classList.toggle('over', sec > ends[total - 1])
    if (auto && running && cur < total - 1 && sec >= ends[cur]) next()
    requestAnimationFrame(tick)
  }

  addEventListener('keydown', e => {
    if (e.ctrlKey || e.metaKey || e.altKey) return
    const k = e.key
    if (k === 'ArrowRight' || k === 'PageDown' || k === ' ' || k === 'Enter') { e.preventDefault(); next() }
    else if (k === 'ArrowLeft' || k === 'PageUp' || k === 'Backspace') { e.preventDefault(); prev() }
    else if (k === 'Home') go(0)
    else if (k === 'End') go(total - 1)
    else if (/^[1-9]$/.test(k)) go(+k - 1)
    else if (k === 'n' || k === 'N' || k === 'т' || k === 'Т') $('#notes').classList.toggle('on')
    else if (k === 't' || k === 'T' || k === 'е' || k === 'Е') document.body.classList.toggle('hud')
    else if (k === 'p' || k === 'P' || k === 'з' || k === 'З') { running = !running; auto = auto && running }
    else if (k === 'r' || k === 'R' || k === 'к' || k === 'К') { elapsed = 0; running = false; auto = false }
    else if (k === 'a' || k === 'A' || k === 'ф' || k === 'Ф') { auto = !auto; if (auto) { elapsed = cur > 0 ? ends[cur - 1] * 1000 : 0; running = true } }
    else if (k === 'f' || k === 'F' || k === 'а' || k === 'А') { document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen?.() }
    else if (k === '?' || k === 'h' || k === 'H' || k === 'р' || k === 'Р') $('#help').classList.toggle('on')
    else if (k === 'Escape') { $('#help').classList.remove('on'); $('#notes').classList.remove('on') }
  })
  stage.addEventListener('click', e => {
    if (e.target.closest('#notes, #help, .topbar, .statusbar')) return
    const r = stage.getBoundingClientRect()
    ;(e.clientX - r.left) / r.width < 0.2 ? prev() : next()
  })
  addEventListener('hashchange', () => go((parseInt(location.hash.slice(1), 10) || 1) - 1, { silent: true }))

  // ------------------------------------------------------------------ boot
  $$('[data-fig]').forEach(buildFigure)
  buildPulseChart($('#pulse-chart'))
  const terminal = $('#term-canvas')
  drawTerminal(terminal)
  document.fonts?.ready.then(() => drawTerminal(terminal))
  window.Deck = { go, next, prev, slides, durs, ends, get index() { return cur }, $, $$ }
  go((parseInt(location.hash.slice(1), 10) || 1) - 1, { silent: true })
  requestAnimationFrame(tick)
})()
