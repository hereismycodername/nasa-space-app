/* AstroCheck pitch deck: motion layer.
   deck.css holds the finished look of every element. A scene animates *from* a start state (Web Animations with
   fill: backwards) and lets go, so nothing stays hidden if a script fails. Leaving a slide cancels its animations
   and undoes every text or class the scene changed. Scenes are keyed by the slide's class name.
   Keys: M turns motion off/on, 0 replays the current slide. `?static` in the URL starts with motion off. */
(() => {
  'use strict'
  const D = window.Deck
  const body = document.body
  if (!D || !Element.prototype.animate) { body.classList.remove('booting'); return }

  const stage = document.getElementById('stage')
  const E = { out: 'cubic-bezier(.16,1,.3,1)', inout: 'cubic-bezier(.65,0,.35,1)', back: 'cubic-bezier(.34,1.56,.64,1)', in: 'cubic-bezier(.5,0,.9,.4)', lin: 'linear' }
  const NS = 'http://www.w3.org/2000/svg'
  const mk = (tag, cls) => { const e = document.createElement(tag); if (cls) e.className = cls; return e }
  const smk = (tag, attrs = {}) => { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); return e }
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v))
  const easeInOut = t => (t < .5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2)
  const easeOut = t => 1 - Math.pow(1 - t, 3)
  // a point inside `node` (fractions of its box) in the coordinates of `root`, in unscaled stage pixels
  const pointIn = (node, root, fx = .5, fy = .5) => {
    const s = stage.getBoundingClientRect().width / 1920
    const a = node.getBoundingClientRect(), b = root.getBoundingClientRect()
    return { x: (a.left - b.left + a.width * fx) / s, y: (a.top - b.top + a.height * fy) / s }
  }
  // shorthand start state -> keyframe (individual transform properties, so stylesheet transforms stay intact)
  const frame = f => {
    const k = {}
    for (const key in f) {
      const v = f[key]
      if (key === 'o') k.opacity = v
      else if (key === 'x' || key === 'y') k.translate = `${f.x || 0}px ${f.y || 0}px`
      else if (key === 's') k.scale = String(v)
      else if (key === 'sx' || key === 'sy') k.scale = `${f.sx ?? 1} ${f.sy ?? 1}`
      else if (key === 'blur') k.filter = `blur(${v}px)`
      else k[key] = v
    }
    return k
  }

  // ------------------------------------------------------------------ text splitting (done once, at boot)
  function splitWords(root) {
    const walk = (src, dst, hook) => {
      for (const n of [...src.childNodes]) {
        if (n.nodeType === 3) {
          for (const tok of n.textContent.split(/(\s+)/)) {
            if (!tok) continue
            if (/^\s+$/.test(tok)) { dst.append(' '); continue }
            const w = mk('span', hook ? 'w em' : 'w'), inner = mk('span', 'w-i')
            inner.textContent = tok
            w.append(inner)
            if (hook) w.append(mk('i', 'wipe'))
            dst.append(w)
          }
        } else if (n.nodeName === 'BR') dst.append(document.createElement('br'))
        else if (n.nodeType === 1) { const c = n.cloneNode(false); dst.append(c); walk(n, c, hook || n.nodeName === 'EM') }
      }
    }
    const frag = document.createDocumentFragment()
    walk(root, frag, false)
    root.setAttribute('aria-label', root.textContent.replace(/\s+/g, ' ').trim())
    root.replaceChildren(frag)
  }
  // every text node becomes one wrapper holding a span per visible character
  function splitChars(root, cls = 'tc') {
    if (root._chars) return root._chars
    const out = []
    const walk = node => {
      for (const n of [...node.childNodes]) {
        if (n.nodeType === 3) {
          if (!n.textContent.trim()) continue
          const wrap = mk('span', 'tw')
          for (const ch of n.textContent) {
            if (/\s/.test(ch)) wrap.append(ch)
            else { const s = mk('span', cls); s.textContent = ch; wrap.append(s); out.push(s) }
          }
          n.replaceWith(wrap)
        } else if (n.nodeType === 1) walk(n)
      }
    }
    walk(root)
    root._chars = out
    return out
  }

  // ------------------------------------------------------------------ scene: everything one slide has in flight
  class Scene {
    constructor(root, t0 = 0) { this.root = root; this.t0 = t0; this.anims = []; this.timers = new Set(); this.rafs = new Set(); this.undo = []; this.dead = false; this.frozen = false }
    q(s) { return typeof s === 'string' ? this.root.querySelector(s) : s }
    qa(s) { return !s ? [] : typeof s === 'string' ? [...this.root.querySelectorAll(s)] : s instanceof Element ? [s] : [...s] }
    // keyframes on one or many elements; by default the start state also covers the wait before `at`
    play(target, frames, o = {}) {
      const { at = 0, dur = 700, ease = E.out, stagger = 0, fill = 'backwards', iterations = 1 } = o
      return this.qa(target).map((e, i) => {
        const a = e.animate(frames, { delay: Math.max(0, this.t0 + at + i * stagger), duration: dur, easing: ease, fill, iterations })
        this.anims.push(a)
        return a
      })
    }
    // from a start state to the stylesheet state. One per property per element: its start state holds from time zero.
    from(target, f, o) { return this.play(target, [{ ...frame(f), offset: 0 }], o) }
    // a passing effect on top of whatever the element shows (no hold before or after)
    fx(target, frames, o = {}) { return this.play(target, frames, { fill: 'none', ...o }) }
    hide(target, until) { return this.play(target, [{ opacity: 0 }, { opacity: 0 }], { at: -this.t0, dur: Math.max(1, this.t0 + until), fill: 'none', ease: E.lin }) }
    at(ms, fn) {
      const id = setTimeout(() => { this.timers.delete(id); if (!this.dead && !this.frozen) fn() }, this.t0 + ms)
      this.timers.add(id)
    }
    every(ms, fn, start = 0) {
      const tick = () => { if (this.dead || this.frozen) return; fn(); this.timers.add(setTimeout(tick, ms)) }
      this.timers.add(setTimeout(tick, this.t0 + start))
    }
    tween(dur, fn, done) {
      const start = performance.now()
      const step = now => {
        if (this.dead || this.frozen) return
        const t = clamp((now - start) / dur, 0, 1)
        fn(t)
        if (t < 1) this.rafs.add(requestAnimationFrame(step))
        else if (done) done()
      }
      this.rafs.add(requestAnimationFrame(step))
    }
    // a change to text or classes, undone when the slide is left
    set(apply, undo) { apply(); this.undo.push(undo) }

    wipe(target, o = {}) {
      const { dir = 'right', at = 0, dur = 800, ease = E.out, stagger = 0, edge = true } = o
      const start = { right: 'inset(0 100% 0 0)', left: 'inset(0 0 0 100%)', down: 'inset(0 0 100% 0)', up: 'inset(100% 0 0 0)', y: 'inset(50% 0 50% 0)' }[dir]
      this.qa(target).forEach((e, i) => {
        const t = at + i * stagger
        this.play(e, [{ clipPath: start }, { clipPath: 'inset(0 0 0 0)' }], { at: t, dur, ease })
        if (!edge || dir === 'y') return
        // a bright line rides the front of the wipe: the panel is printed, not faded
        if (getComputedStyle(e).position === 'static') { e.style.position = 'relative'; this.undo.push(() => { e.style.position = '' }) }
        const line = mk('i', 'wipe-edge')
        const horiz = dir === 'right' || dir === 'left'
        const w = e.offsetWidth, h = e.offsetHeight
        line.style.cssText = horiz ? `top:0;bottom:0;width:2px;left:${dir === 'right' ? -2 : w}px` : `left:0;right:0;height:2px;top:${dir === 'down' ? -2 : h}px`
        const to = horiz ? `${dir === 'right' ? w : -w}px 0` : `0 ${dir === 'down' ? h : -h}px`
        e.append(line)
        this.undo.push(() => line.remove())
        this.play(line, [{ translate: '0 0' }, { translate: to }], { at: t, dur, ease })
        this.play(line, [{ opacity: 1 }, { opacity: 1, offset: .6 }, { opacity: 0 }], { at: t, dur, ease: E.lin })
      })
    }
    // characters appear one after another, each lighting up as it lands
    type(target, o = {}) {
      const { at = 0, cps = 60, glow = '#d6e4ff' } = o
      let end = at
      this.qa(target).forEach(e => {
        const chars = splitChars(e), step = 1000 / cps
        this.hide(e, at) // the element's own marks (a caption's square) wait for the first character
        chars.forEach((c, i) => {
          const t = this.t0 + at + i * step
          this.anims.push(c.animate([{ opacity: 0 }, { opacity: 0 }], { duration: Math.max(1, t), fill: 'none' }))
          if (glow) this.anims.push(c.animate([{ color: glow, offset: 0 }], { delay: Math.max(0, t), duration: 340, easing: E.lin }))
        })
        end = Math.max(end, at + chars.length * step)
      })
      return end
    }
    // plain words rise out of their mask; hook words (inside <em>) are revealed by a block that wipes across
    words(target, o = {}) {
      const e = this.q(target)
      const { at = 0, stagger = 55, dur = 850, gap = 140, hookStagger = 120 } = o
      if (!e) return at
      const all = [...e.querySelectorAll('.w')]
      const plain = all.filter(w => !w.classList.contains('em')), hook = all.filter(w => w.classList.contains('em'))
      plain.forEach((w, i) => this.play(w.firstChild, [{ translate: '0 118%', offset: 0 }], { at: at + i * stagger, dur }))
      const t = at + plain.length * stagger + (plain.length ? gap : 0)
      hook.forEach((w, i) => {
        const s = t + i * hookStagger
        this.hide(w.firstChild, s + 292)
        this.play(w.lastChild, [
          { scale: '0 1', transformOrigin: '0 50%', easing: E.inout },
          { scale: '1 1', transformOrigin: '0 50%', offset: .46 },
          { scale: '1 1', transformOrigin: '100% 50%', offset: .47, easing: E.inout },
          { scale: '0 1', transformOrigin: '100% 50%' },
        ], { at: s, dur: 640, ease: E.lin })
      })
      return t + Math.max(0, hook.length - 1) * hookStagger + (hook.length ? 640 : 0)
    }
    // a number runs to its value; the text node is restored when the run ends or the slide is left
    count(target, o) {
      const e = this.q(target)
      if (!e) return
      const { from = 0, to, at = 0, dur = 1100, dec = 0, suffix = '', curve = easeOut } = o
      const node = [...e.childNodes].find(n => n.nodeType === 3) || e.insertBefore(document.createTextNode(''), e.firstChild)
      const orig = node.textContent
      const show = v => { node.textContent = v.toFixed(dec).replace('.', ',') + suffix }
      this.set(() => show(from), () => { node.textContent = orig })
      this.at(at, () => this.tween(dur, t => show(from + (to - from) * curve(t)), () => { node.textContent = orig }))
    }
    freeze() { this.frozen = true; this.timers.forEach(clearTimeout); this.rafs.forEach(cancelAnimationFrame); this.timers.clear(); this.rafs.clear() }
    kill() {
      this.freeze()
      this.dead = true
      for (const a of this.anims) { try { a.cancel() } catch { /* already gone */ } }
      for (const u of this.undo.reverse()) { try { u() } catch (err) { console.error(err) } }
      this.anims = []
      this.undo = []
    }
  }

  // ------------------------------------------------------------------ shared moves
  function headline(sc, at = 0) {
    const kicker = sc.q('.kicker'), h1 = sc.q('h1'), lead = sc.q('p.lead') // not the figure's leader lines
    let t = at
    if (kicker) sc.type(kicker, { at: at + 60, cps: 60 })
    if (h1) t = sc.words(h1, { at: at + 140 })
    if (lead) sc.from(lead, { o: 0, y: 18 }, { at: Math.max(at + 500, t - 550), dur: 800 })
    return t
  }

  // the body is printed top to bottom: skin, a scan that uncovers the skeleton, organs behind the scan line,
  // then status outlines and the callouts
  function assembleFigure(sc, panel, o = {}) {
    const { at = 0, scan = true } = o
    const A = window.ANATOMY
    const box = panel.querySelector('.figbox'), fig = panel.querySelector('.fig')
    if (!A || !box || !fig) return at
    const H = box.offsetHeight
    sc.wipe(panel, { dir: 'down', at, dur: 900 })
    sc.type(panel.querySelector('.cap'), { at: at + 350, cps: 50 })
    sc.from(panel.querySelectorAll('.figmarks .tick'), { o: 0 }, { at: at + 300, dur: 300, stagger: 22 })
    sc.from(panel.querySelectorAll('.figmarks text'), { o: 0 }, { at: at + 700, dur: 600 })
    sc.from(fig.querySelector('.l-body'), { o: 0 }, { at: at + 350, dur: 900, ease: E.inout })
    const t0 = at + (scan ? 1000 : 520), sweep = scan ? 1500 : 0
    const skel = fig.querySelector('.l-skel')
    if (scan) {
      sc.fx(panel.querySelector('.scanline'), [{ translate: '0 0', opacity: 0 }, { opacity: 1, offset: .06 }, { opacity: 1, offset: .93 }, { translate: `0 ${H}px`, opacity: 0 }], { at: t0, dur: sweep, ease: E.lin })
      sc.play(skel, [{ clipPath: 'inset(0 0 100% 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: t0, dur: sweep, ease: E.lin })
      sc.play(skel, [{ opacity: 1, offset: 0 }, { opacity: 1, offset: .62 }], { at: t0, dur: sweep * 1.9, ease: E.lin })
      sc.from(fig.querySelector('.l-glass'), { o: 0 }, { at: t0 + sweep * .5, dur: 900 })
    } else {
      sc.from(skel, { o: 0 }, { at: t0, dur: 600 })
      sc.from(fig.querySelector('.l-glass'), { o: 0 }, { at: t0 + 100, dur: 700 })
    }
    ;[...fig.querySelectorAll('.org')].forEach((img, i) => {
      const cy = (parseFloat(img.style.top) + parseFloat(img.style.height) / 2) / A.h
      sc.from(img, { o: 0, s: .55 }, { at: scan ? t0 + cy * sweep + 60 : t0 + i * 26, dur: 560, ease: E.back })
    })
    const tOut = t0 + sweep + (scan ? 150 : 700)
    sc.play(fig.querySelectorAll('.out.on'), [{ opacity: 0 }, { opacity: .92, offset: .25 }, { opacity: .15, offset: .5 }, { opacity: .92, offset: .75 }, { opacity: .92 }], { at: tOut, dur: 700, ease: E.lin })
    const order = [...panel.querySelectorAll('.callout')].sort((a, b) => parseFloat(a.style.top) - parseFloat(b.style.top))
    order.forEach((lab, i) => {
      const sys = lab.dataset.sys, t = tOut + 100 + i * 110
      const g = panel.querySelector(`.co[data-sys="${sys}"]`)
      sc.from(panel.querySelector(`.marker[data-sys="${sys}"]`), { o: 0, s: 0 }, { at: t, dur: 420, ease: E.back })
      if (g) {
        sc.play(g.querySelector('path'), [{ strokeDasharray: '1 1', strokeDashoffset: 1 }, { strokeDasharray: '1 1', strokeDashoffset: 0 }], { at: t + 120, dur: 480, ease: E.inout })
        sc.from(g.querySelector('line'), { o: 0 }, { at: t + 560, dur: 200 })
      }
      sc.from(lab, { o: 0, x: lab.classList.contains('left') ? 14 : -14 }, { at: t + 480, dur: 520 })
    })
    return tOut + 100 + order.length * 110 + 600
  }

  // ------------------------------------------------------------------ scenes
  const scenes = {}

  // 1 · title: the instrument boots
  scenes['s-title'] = sc => {
    sc.from('.kicker .sq', { o: 0, s: 0 }, { at: 80, dur: 420, ease: E.back })
    sc.type('.kicker', { at: 160, cps: 62 })
    sc.from('.wordmark .ch', { o: 0, x: 46, blur: 14 }, { at: 380, dur: 1050, stagger: 62 })
    sc.fx('.wordmark .ch', [{ color: '#e6e9ed' }, { color: '#8fb8ff', offset: .5 }, { color: '#e6e9ed' }], { at: 1750, dur: 520, stagger: 48, ease: E.lin })
    sc.words('.tagline', { at: 1250 })
    sc.wipe('.selftest', { at: 2000, dur: 650 })
    sc.qa('.selftest li').forEach((li, i) => {
      const t = 2300 + i * 430, value = li.querySelector('em')
      sc.type(li.querySelector('b'), { at: t, cps: 40 })
      sc.type(li.querySelector('.lbl'), { at: t + 90, cps: 55 })
      sc.from(li.querySelector('i'), { sx: 0 }, { at: t + 200, dur: 520, ease: E.inout })
      sc.from(value, { o: 0, x: 10 }, { at: t + 620, dur: 300 })
      sc.fx(value, [{ color: '#ffffff', offset: 0 }], { at: t + 620, dur: 600, ease: E.lin })
    })
    assembleFigure(sc, sc.q('.figpanel'), { at: 450 })
  }

  // 2 · problem: the question travels to Earth and back while the minutes run
  scenes['s-problem'] = sc => {
    headline(sc)
    const row = sc.q('.linkpanel .row'), num = sc.q('.delay .num'), msg = sc.q('.linkpanel .msg'), reply = sc.q('.linkpanel .msg.reply')
    sc.wipe('.linkpanel', { at: 900, dur: 800 })
    sc.from(num, { o: 0, y: 34 }, { at: 1250, dur: 800 })
    sc.from('.delay span', { o: 0, x: -18 }, { at: 1500, dur: 700 })
    sc.from('.state .tag', { o: 0, s: .7 }, { at: 1750, dur: 450, ease: E.back })
    sc.from('.node.earth', { o: 0, x: -20 }, { at: 1500, dur: 600 })
    sc.from('.node.ship', { o: 0, x: 20 }, { at: 1650, dur: 600 })
    sc.play('.track', [{ clipPath: 'inset(0 0 0 100%)' }, { clipPath: 'inset(0 0 0 0)' }], { at: 1800, dur: 900, ease: E.inout })
    sc.type(msg, { at: 2200, cps: 46 })

    const T = 3300, LEG = 2800, CYCLE = LEG * 2
    const digits = num.firstChild
    sc.set(() => { digits.textContent = '0' }, () => { digits.textContent = '40' })
    sc.at(T, () => sc.tween(CYCLE, t => { digits.textContent = String(Math.round(40 * t)) }))
    const x0 = 170, x1 = row.offsetWidth - 236
    const packet = sc.q('.packet')
    sc.hide(packet, T)
    sc.play(packet, [
      { left: x1 + 'px', scale: '-1 1', offset: 0 }, { left: x0 + 'px', scale: '-1 1', offset: .47 }, { left: x0 + 'px', scale: '-1 1', offset: .5 },
      { left: x0 + 'px', scale: '1 1', offset: .5 }, { left: x1 + 'px', scale: '1 1', offset: .97 }, { left: x1 + 'px', scale: '1 1', offset: 1 },
    ], { at: T, dur: CYCLE, ease: E.lin, iterations: Infinity })
    sc.play('.node.earth i', [{ scale: 1, offset: 0 }, { scale: 1, offset: .455 }, { scale: 1.8, offset: .485 }, { scale: 1, offset: .57 }, { scale: 1, offset: 1 }], { at: T, dur: CYCLE, ease: E.lin, iterations: Infinity })
    sc.play('.node.ship i', [{ scale: 1.8, offset: 0 }, { scale: 1, offset: .08 }, { scale: 1, offset: .955 }, { scale: 1.8, offset: 1 }], { at: T, dur: CYCLE, ease: E.lin, iterations: Infinity })
    // forty minutes later the answer lands
    sc.fx(num, [{ scale: 1 }, { scale: 1.16, offset: .3 }, { scale: 1 }], { at: T + CYCLE, dur: 650 })
    sc.fx('.state .tag', [{ opacity: 1 }, { opacity: .2, offset: .25 }, { opacity: 1, offset: .5 }, { opacity: .2, offset: .75 }, { opacity: 1 }], { at: T + CYCLE, dur: 800, ease: E.lin })
    sc.play(msg, [{ opacity: 1 }, { opacity: 0 }], { at: T + CYCLE, dur: 300, fill: 'both' })
    sc.play(reply, [{ opacity: 1 }, { opacity: 1 }], { at: T + CYCLE + 280, dur: 10, fill: 'forwards' })
    sc.type(reply, { at: T + CYCLE + 300, cps: 46 })

    sc.type('.hz-cap', { at: 3500, cps: 60 })
    const tiles = sc.qa('.hazard')
    tiles.forEach((h, i) => {
      const t = 3900 + i * 140
      sc.wipe(h, { at: t, dur: 700 })
      sc.type(h.querySelector('.n'), { at: t + 250, cps: 50 })
      sc.from(h.querySelector('h3'), { o: 0, y: 16 }, { at: t + 300, dur: 600 })
      sc.from(h.querySelector('p'), { o: 0, y: 16 }, { at: t + 400, dur: 600 })
    })
    // the five hazards are named one by one
    let k = 0
    sc.every(950, () => { tiles.forEach((h, i) => h.classList.toggle('hot', i === k % tiles.length)); k++ }, 10800)
    sc.undo.push(() => tiles.forEach(h => h.classList.remove('hot')))
  }

  // 3 · solution: a camera walks the four steps across the real screen
  scenes['s-solution'] = sc => {
    headline(sc)
    const view = sc.q('.shot .view'), cam = sc.q('.cam'), focus = sc.q('.focus'), steps = sc.qa('.step')
    sc.play('.shot', [{ opacity: 0, transform: 'perspective(1800px) translate3d(150px, 24px, 0) rotateY(-18deg) rotateX(5deg) scale(.9)', offset: 0 }], { at: 450, dur: 1400 })
    steps.forEach((st, i) => {
      const t = 1050 + i * 150
      sc.wipe(st, { at: t, dur: 650 })
      sc.from(st.querySelector('.n'), { o: 0 }, { at: t + 250, dur: 400 })
      sc.from(st.querySelector('div'), { o: 0, x: -14 }, { at: t + 220, dur: 600 })
    })
    if (!view || !cam || !focus) return
    const label = focus.querySelector('span')
    const stops = [
      { r: [10, 136, 282, 471], z: 1.12, text: '01 · Наблюдение' },
      { r: [323, 196, 738, 158], z: 1.3, text: '02 · Объяснение' },
      { r: [337, 500, 470, 107], z: 1.5, text: '03 · Действие' },
      { r: [818, 462, 232, 86], z: 1.7, text: '04 · Эскалация' },
    ]
    const V = { w: view.offsetWidth, h: view.offsetHeight }
    const box = ([x, y, w, h]) => { focus.style.left = x + 'px'; focus.style.top = y + 'px'; focus.style.width = w + 'px'; focus.style.height = h + 'px' }
    const aim = i => {
      const { r, z, text } = stops[i]
      box(r)
      focus.style.opacity = 1
      label.textContent = text
      const tx = clamp(V.w / 2 - (r[0] + r[2] / 2) * z, V.w - V.w * z, 0), ty = clamp(V.h / 2 - (r[1] + r[3] / 2) * z, V.h - V.h * z, 0)
      cam.style.transform = `translate(${tx}px, ${ty}px) scale(${z})`
      steps.forEach((s, n) => s.classList.toggle('on', n === i))
    }
    const rest = () => { cam.style.transform = 'none'; focus.style.opacity = 0; steps.forEach(s => s.classList.remove('on')) }
    const home = focus.style.cssText, homeText = label.textContent
    sc.set(() => { rest(); box(stops[0].r) }, () => {
      focus.style.cssText = home
      label.textContent = homeText
      cam.style.transform = ''
      steps.forEach((s, n) => s.classList.toggle('on', n === 1))
    })
    const HOLD = 3300
    const tour = () => {
      stops.forEach((_, i) => sc.at(i * HOLD, () => aim(i)))
      sc.at(stops.length * HOLD, rest)
      sc.at(stops.length * HOLD + 2400, tour)
    }
    sc.at(2500, tour)
  }

  // 4 · body map: a cursor flips the layers, clicks the head, logs a symptom
  scenes['s-body'] = sc => {
    const slide = sc.root, panel = sc.q('.figpanel'), cur = sc.q('.cursor'), ring = sc.q('.click-ring')
    if (!panel) return
    const fig = panel.querySelector('.fig'), figure = panel.figure, line = panel.querySelector('.scanline')
    assembleFigure(sc, panel, { at: 0, scan: false })
    sc.from('.layer-tabs', { o: 0, y: -12 }, { at: 400, dur: 600 })
    headline(sc, 150)
    const card = sc.q('.detail'), tiles = sc.qa('.tile'), btn = sc.q('.detail .btn')
    sc.wipe(card, { at: 8300, dur: 800 })
    sc.from('.detail .head h3', { o: 0, x: -16 }, { at: 8600, dur: 600 })
    sc.from('.detail .head .tag', { o: 0, s: .7 }, { at: 8850, dur: 450, ease: E.back })
    tiles.forEach((t, i) => {
      const s = 8900 + i * 150
      sc.wipe(t, { at: s, dur: 600, edge: false })
      sc.from(t.querySelector('span'), { o: 0 }, { at: s + 150, dur: 400 })
      sc.from(t.querySelector('.num'), { o: 0, y: 16 }, { at: s + 200, dur: 600 })
    })
    sc.count(tiles[0].querySelector('.num'), { from: 7.5, to: 4.1, dec: 1, at: 9150, dur: 1000 })
    sc.count(tiles[1].querySelector('.num'), { from: 5, to: 2, at: 9300, dur: 1000 })
    sc.from(btn, { o: 0, y: 12 }, { at: 9500, dur: 500 })
    sc.from('.detail .saved', { o: 0, x: -12 }, { at: 12300, dur: 500 })
    sc.from('.points li', { o: 0, y: 18 }, { at: 13000, dur: 700, stagger: 280 })
    if (!cur || !ring || !figure) return

    // the cursor
    let tip = { x: 0, y: 0 }
    const target = (node, fx, fy) => { const p = pointIn(node, slide, fx, fy); return { x: p.x - 4, y: p.y - 2 } }
    const moveTo = (node, fx = .5, fy = .5) => { tip = target(node, fx, fy); cur.style.translate = `${tip.x}px ${tip.y}px` }
    const jump = (x, y) => { cur.style.transition = 'none'; cur.style.translate = `${x}px ${y}px`; cur.getBoundingClientRect(); cur.style.transition = '' }
    const click = () => {
      ring.style.translate = `${tip.x + 4}px ${tip.y + 2}px`
      sc.fx(ring, [{ opacity: 1, scale: .3 }, { opacity: 0, scale: 1.5 }], { dur: 480 })
      sc.fx(cur, [{ scale: 1 }, { scale: .8, offset: .4 }, { scale: 1 }], { dur: 260, ease: E.inout })
    }
    const homeAt = cur.style.translate
    sc.set(() => { cur.style.opacity = 0; tip = target(panel, .8, .22); jump(tip.x, tip.y) }, () => { cur.style.opacity = ''; const [x, y] = homeAt.split(' ').map(parseFloat); jump(x, y) })
    sc.at(1650, () => { cur.style.opacity = 1 })

    // layers: organs -> skeleton -> X-ray -> organs
    const tabs = {}
    sc.qa('.layer-tabs span').forEach(t => { tabs[t.dataset.mode] = t })
    const H = panel.querySelector('.figbox').offsetHeight
    const switchTo = (mode, t) => {
      sc.at(t, () => moveTo(tabs[mode], .5, .62))
      sc.at(t + 880, () => { click(); figure.setMode(mode) })
      sc.fx(line, [{ translate: '0 0', opacity: 0 }, { opacity: 1, offset: .1 }, { opacity: 1, offset: .9 }, { translate: `0 ${H}px`, opacity: 0 }], { at: t + 900, dur: 850, ease: E.inout })
    }
    switchTo('skeleton', 1800)
    switchTo('xray', 3600)
    switchTo('organs', 5400)
    sc.undo.push(() => figure.setMode('organs'))

    // the head: hover, click, the rest of the body steps back
    const outs = [...fig.querySelectorAll('.out')]
    const outline = (sys, kind) => outs.forEach(o => { if (o.dataset.sys === sys) o.classList.toggle('on', o.classList.contains('out-' + kind)) })
    const dimmable = [...fig.querySelectorAll('.org'), ...panel.querySelectorAll('.callout, .marker'), ...outs]
    sc.undo.push(() => { dimmable.forEach(n => n.classList.remove('dim')); outline('brain', 'warn') })
    const brain = panel.querySelector('.marker[data-sys="brain"]')
    sc.at(7000, () => moveTo(brain, 2.4, 2.2))
    sc.at(7750, () => outline('brain', 'white'))
    sc.at(8050, () => { click(); dimmable.forEach(n => n.classList.toggle('dim', n.dataset.sys !== 'brain')) })

    // a symptom is logged with one click
    const pain = tiles[2].querySelector('.num'), painText = pain.firstChild
    sc.set(() => { painText.textContent = '—' }, () => { painText.textContent = '5'; pain.querySelector('.caret')?.remove(); btn.classList.remove('press') })
    sc.at(10100, () => moveTo(btn, .72, .6))
    sc.at(11100, () => { click(); btn.classList.add('press'); sc.at(190, () => btn.classList.remove('press')) })
    sc.at(11350, () => { painText.textContent = ''; painText.after(mk('i', 'caret')) })
    sc.at(11850, () => { painText.textContent = '5' })
    sc.at(12250, () => { pain.querySelector('.caret')?.remove(); sc.fx(tiles[2], [{ borderColor: '#3a7bef', backgroundColor: '#3a7bef33', offset: 0 }], { dur: 900 }) })
    sc.at(12900, () => { cur.style.opacity = 0 })
  }

  // 5 · assessment: three short nights trip the rule, the next step follows
  scenes['s-assess'] = sc => {
    headline(sc)
    const metric = sc.q('.metric'), tag = sc.q('.metric .tag')
    sc.wipe(metric, { at: 700, dur: 800 })
    sc.type('.metric .cap', { at: 1000, cps: 50 })
    sc.set(() => { tag.classList.remove('warn'); tag.textContent = 'Без срабатываний' }, () => { tag.classList.add('warn'); tag.textContent = 'Требует внимания' })
    sc.from(tag, { o: 0, s: .7 }, { at: 1250, dur: 450, ease: E.back })
    sc.from('.val .num', { o: 0, y: 26 }, { at: 1150, dur: 700 })
    sc.from('.val small', { o: 0 }, { at: 1400, dur: 500 })
    sc.from('.bars i', { sy: 0 }, { at: 1400, dur: 620, stagger: 230 })
    sc.from('.days span', { o: 0, y: 8 }, { at: 1450, dur: 400, stagger: 230 })
    sc.play('.bars .thr', [{ clipPath: 'inset(-30px 100% 0 0)' }, { clipPath: 'inset(-30px 0 0 0)' }], { at: 1500, dur: 900, ease: E.inout })
    sc.count('.val .num', { from: 7.6, to: 4.1, dec: 1, at: 2350, dur: 1100, curve: t => t })
    const FLIP = 3400
    sc.fx(tag, [{ scale: '1 1' }, { scale: '1 0', offset: .45 }, { scale: '1 0', offset: .55 }, { scale: '1 1' }], { at: FLIP, dur: 440, ease: E.inout })
    sc.at(FLIP + 220, () => { tag.classList.add('warn'); tag.textContent = 'Требует внимания' })
    sc.fx(metric, [{ boxShadow: '0 0 0 0 #f2b63200', borderColor: '#232932' }, { boxShadow: '0 0 0 3px #f2b63255', borderColor: '#f2b632', offset: .3 }, { boxShadow: '0 0 0 0 #f2b63200', borderColor: '#232932' }], { at: FLIP + 200, dur: 900 })
    sc.fx('.bars i.low', [{ opacity: 1 }, { opacity: .4, offset: .5 }, { opacity: 1 }], { at: FLIP + 200, dur: 700, stagger: 90, ease: E.lin })
    sc.from('.val span', { o: 0, x: 14 }, { at: FLIP + 250, dur: 600 })

    sc.play('.connector', [{ clipPath: 'inset(0 100% 0 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: 3800, dur: 600, ease: E.inout })
    sc.wipe('.guide', { at: 4150, dur: 850 })
    sc.type('.guide .cap', { at: 4450, cps: 55 })
    sc.from('.guide .ruleid', { o: 0 }, { at: 4700, dur: 500 })
    sc.qa('.guide dl > div').forEach((rowEl, i) => {
      const t = 4850 + i * 620
      if (rowEl.classList.contains('key')) sc.play(rowEl, [{ clipPath: 'inset(0 100% 0 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: t, dur: 700, ease: E.inout })
      sc.from(rowEl.querySelector('dt'), { o: 0, x: -12 }, { at: t + 60, dur: 500 })
      sc.from(rowEl.querySelector('dd'), { o: 0, y: 14 }, { at: t + 140, dur: 650 })
    })
    sc.from('.guide .src', { o: 0, s: .7 }, { at: 4850 + 3 * 620 + 300, dur: 420, stagger: 150, ease: E.back })

    sc.type('.legend .cap', { at: 7500, cps: 50 })
    sc.from('.legend .tag', { o: 0, s: .6 }, { at: 7650, dur: 450, stagger: 170, ease: E.back })
    sc.from('.alert', { o: 0, x: -50 }, { at: 8500, dur: 800 })
    sc.fx('.alert', [{ boxShadow: '0 0 0 0 #ff5b4d00' }, { boxShadow: '0 0 0 4px #ff5b4d55', offset: .2 }, { boxShadow: '0 0 0 0 #ff5b4d00', offset: .5 }, { boxShadow: '0 0 0 4px #ff5b4d55', offset: .7 }, { boxShadow: '0 0 0 0 #ff5b4d00' }], { at: 9000, dur: 1000, ease: E.lin })
  }

  // 6 · honest data: a pen draws the week and stops where there is nothing to draw
  scenes['s-data'] = sc => {
    headline(sc)
    const host = sc.q('#pulse-chart'), M = host && host._motion
    sc.wipe('.chart', { at: 600, dur: 850 })
    sc.type('.chart .cap', { at: 900, cps: 55 })
    sc.from('.chart .period', { o: 0 }, { at: 1100, dur: 500 })
    if (M && M.pts.length > 4) {
      sc.from(host.querySelectorAll('.gridl'), { o: 0 }, { at: 1000, dur: 500, stagger: 40 })
      sc.from(host.querySelectorAll('.axis'), { o: 0 }, { at: 1150, dur: 500, stagger: 30 })
      const pts = M.pts
      let gi = 1
      for (let i = 1; i < pts.length; i++) if (pts[i][0] - pts[i - 1][0] > 20) { gi = i; break }
      const A = pts.slice(0, gi), C = pts.slice(gi)
      const yAt = (seg, x) => {
        let lo = 0, hi = seg.length - 1
        while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (seg[mid][0] <= x) lo = mid; else hi = mid }
        const [xa, ya] = seg[lo], [xb, yb] = seg[hi]
        return xb === xa ? ya : ya + (yb - ya) * clamp((x - xa) / (xb - xa), 0, 1)
      }
      const reveal = x => M.rect.setAttribute('width', x)
      const pen = (x, y) => { M.pen.setAttribute('cx', x); M.pen.setAttribute('cy', y) }
      sc.set(() => { reveal(0); pen(A[0][0], A[0][1]); M.pen.style.opacity = 0 }, () => { reveal(1300); M.pen.style.opacity = ''; M.pen.classList.remove('wait', 'live'); pen(-20, -20) })
      const tA = 1500, dA = 1350, tGap = tA + dA, dGap = 1100, tC = tGap + dGap, dC = 2300
      const spanA = A[A.length - 1][0] - A[0][0], spanC = C[C.length - 1][0] - C[0][0]
      sc.at(tA, () => { M.pen.style.opacity = 1; sc.tween(dA, t => { const x = A[0][0] + spanA * easeInOut(t); reveal(x + 1); pen(x, yAt(A, x)) }, () => M.pen.classList.add('wait')) })
      const gap = host.querySelector('.g-gap')
      sc.from(gap, { o: 0 }, { at: tGap + 80, dur: 350 })
      sc.fx(gap, [{ opacity: 1 }, { opacity: .25, offset: .25 }, { opacity: 1, offset: .5 }, { opacity: .25, offset: .75 }, { opacity: 1 }], { at: tGap + 430, dur: 520, ease: E.lin })
      sc.at(tC, () => { M.pen.classList.remove('wait'); sc.tween(dC, t => { const x = C[0][0] + spanC * easeOut(t); reveal(x + 1); pen(x, yAt(C, x)) }, () => M.pen.classList.add('live')) })
      // the raised stretch is marked when the pen gets there (easeOut inverted: t = 1 - cbrt(1 - p))
      const zone = host.querySelector('.zone')
      const p = clamp((+zone.getAttribute('x') - C[0][0]) / spanC, 0, 1)
      sc.from(host.querySelector('.g-zone'), { o: 0 }, { at: tC + dC * (1 - Math.cbrt(1 - p)) - 80, dur: 600 })
    }

    sc.type('.fresh .cap', { at: 1300, cps: 55 })
    const items = sc.qa('.fresh .item')
    items.forEach((it, i) => {
      const t = 2300 + i * 480
      sc.wipe(it, { at: t, dur: 700 })
      sc.from(it.querySelector('h3'), { o: 0, x: -12 }, { at: t + 200, dur: 500 })
      sc.from(it.querySelector('.tag'), { o: 0, s: .7 }, { at: t + 380, dur: 420, ease: E.back })
      sc.from(it.querySelector('p'), { o: 0, y: 10 }, { at: t + 320, dur: 600 })
    })
    if (items.length > 1) {
      // card 1: live readings, the age keeps resetting. Card 2: a day passes and the value goes stale.
      const live = items[0].querySelector('.tag'), stale = items[1].querySelector('.tag')
      let n = 0
      sc.set(() => { live.textContent = 'Свежее · 0 с' }, () => { live.textContent = 'Свежее · 2 с' })
      sc.every(1000, () => {
        n = (n + 1) % 3
        live.textContent = `Свежее · ${n} с`
        if (n === 0) sc.fx(live, [{ backgroundColor: '#9aa5b466', offset: 0 }], { dur: 500, ease: E.lin })
      }, 3200)
      sc.set(() => { stale.classList.remove('none'); stale.textContent = 'Свежее · 0 ч' }, () => { stale.classList.add('none'); stale.textContent = 'Устарело · 26 ч' })
      sc.at(2300 + 480 + 700, () => {
        let flipped = false
        sc.tween(1900, t => {
          const h = Math.round(26 * t)
          if (h >= 24 && !flipped) { flipped = true; stale.classList.add('none'); sc.fx(stale, [{ scale: '1 0', offset: 0 }], { dur: 320, ease: E.back }) }
          stale.textContent = `${flipped ? 'Устарело' : 'Свежее'} · ${h} ч`
        })
      })
    }
    sc.qa('.facts li').forEach((li, i) => {
      const t = 6200 + i * 360
      sc.play(li, [{ clipPath: 'inset(0 0 100% 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: t, dur: 600 })
      sc.from(li.querySelector('b'), { o: 0, x: -14 }, { at: t + 120, dur: 600 })
      sc.from(li.querySelector('span'), { o: 0, x: -14 }, { at: t + 220, dur: 600 })
    })
  }

  // 7 · architecture: packets run from the sources through one core to three screens
  const WIRES = [
    [[370, 85], [422, 85], [422, 233], [474, 233]], [[370, 273], [474, 273]], [[370, 461], [422, 461], [422, 313], [474, 313]],
    [[874, 233], [926, 233], [926, 85], [978, 85]], [[874, 273], [978, 273]], [[874, 313], [926, 313], [926, 461], [978, 461]],
  ]
  const along = pts => {
    const len = []
    let total = 0
    for (let i = 1; i < pts.length; i++) { const d = Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]); len.push(d); total += d }
    let acc = 0
    return pts.map((p, i) => { if (i) acc += len[i - 1]; return { translate: `${p[0]}px ${p[1]}px`, offset: i === pts.length - 1 ? 1 : acc / total } })
  }
  scenes['s-arch'] = sc => {
    headline(sc)
    const cols = sc.qa('.arch .col'), core = sc.q('.core')
    const wires = sc.qa('.wires path:not(.wire-hot)'), hot = sc.qa('.wires .wire-hot'), packets = sc.qa('.pk')
    const boxIn = (b, t) => { sc.wipe(b, { at: t, dur: 700 }); sc.from(b.querySelectorAll('.n, h3, p'), { o: 0, x: -12 }, { at: t + 220, dur: 550, stagger: 70 }) }
    const wireIn = (i, t) => {
      sc.from(wires[i], { o: 0 }, { at: t + 300, dur: 400 })
      sc.fx(hot[i], [{ strokeDasharray: '1 1', strokeDashoffset: 1, opacity: 1 }, { strokeDasharray: '1 1', strokeDashoffset: 0, opacity: 1, offset: .7 }, { strokeDasharray: '1 1', strokeDashoffset: 0, opacity: 0 }], { at: t, dur: 800, ease: E.inout })
      const dur = 1500 + (i % 3) * 190, start = t + 500 + (i % 3) * 270
      sc.play(packets[i], along(WIRES[i]), { at: start, dur, ease: E.lin, iterations: Infinity })
      sc.fx(packets[i], [{ opacity: 0 }, { opacity: 1, offset: .12 }, { opacity: 1, offset: .82 }, { opacity: 0 }], { at: start, dur, ease: E.lin, iterations: Infinity })
    }
    if (cols[0]) [...cols[0].querySelectorAll('.src-box')].forEach((b, i) => boxIn(b, 600 + i * 150))
    if (wires.length === 6 && hot.length === 6 && packets.length === 6) [0, 1, 2].forEach(i => wireIn(i, 1300 + i * 110))

    sc.play(core, [{ clipPath: 'inset(50% 0 50% 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: 1900, dur: 900 })
    sc.fx(core, [{ boxShadow: '0 0 0 1px #3a7bef, 0 0 70px #3a7bef77', offset: 0 }], { at: 2800, dur: 1300 })
    sc.type('.core .cap', { at: 2250, cps: 50 })
    sc.from('.core h3', { o: 0, y: 18 }, { at: 2350, dur: 700 })
    sc.play('.core .seg i', [{ backgroundColor: '#292e37', offset: 0 }], { at: 2700, dur: 260, stagger: 65, ease: E.lin })
    sc.from('.core .log li', { o: 0, x: -12 }, { at: 3050, dur: 450, stagger: 200 })
    sc.from('.core ul:not(.log) li', { o: 0, y: 12 }, { at: 3900, dur: 550, stagger: 160 })

    if (wires.length === 6 && hot.length === 6 && packets.length === 6) [3, 4, 5].forEach(i => wireIn(i, 3500 + (i - 3) * 110))
    if (cols[1]) [...cols[1].querySelectorAll('.ui-box')].forEach((b, i) => {
      const t = 3900 + i * 220
      sc.wipe(b, { at: t, dur: 750 })
      sc.from(b.querySelector('.thumb'), { o: 0, s: .92 }, { at: t + 200, dur: 700 })
      sc.from(b.querySelectorAll('.n, h3, p'), { o: 0, x: -12 }, { at: t + 300, dur: 550, stagger: 70 })
    })
    sc.play('#term-canvas', [{ clipPath: 'inset(0 0 100% 0)' }, { clipPath: 'inset(0 0 0 0)' }], { at: 4700, dur: 1300, ease: E.lin })

    sc.wipe('.stats', { at: 6000, dur: 900 })
    const spec = [{ from: 0, to: 12 }, { from: 0, to: 3 }, { from: 0, to: 2 }, { from: 9, to: 0 }, { from: 0, to: 200, suffix: '+' }]
    sc.qa('.stats > div').forEach((st, i) => {
      const t = 6250 + i * 150
      sc.from(st.querySelector('.num'), { o: 0, y: 18 }, { at: t, dur: 600 })
      if (spec[i]) sc.count(st.querySelector('.num'), { ...spec[i], at: t + 100, dur: 1100 })
      sc.from(st.querySelector('span'), { o: 0, y: 10 }, { at: t + 200, dur: 600 })
    })
    sc.fx('.stats .key', [{ backgroundColor: '#3a7bef66', offset: 0 }], { at: 6250 + 3 * 150 + 1250, dur: 1000 })
    // requests keep arriving
    const lines = sc.qa('.core .log li')
    let k = 0
    sc.every(620, () => { const li = lines[k++ % lines.length]; if (!li) return; li.classList.add('hit'); setTimeout(() => li.classList.remove('hit'), 300) }, 4200)
    sc.undo.push(() => lines.forEach(l => l.classList.remove('hit')))
  }

  // 8 · close: four words, one promise
  scenes['s-close'] = sc => {
    sc.type('.kicker', { at: 60, cps: 60 })
    sc.words('h1', { at: 200, stagger: 330, dur: 900, gap: 260 })
    sc.qa('.listp').forEach((p, n) => {
      const t = 2400 + n * 220
      sc.wipe(p, { at: t, dur: 800 })
      sc.type(p.querySelector('.cap'), { at: t + 300, cps: 50 })
      ;[...p.querySelectorAll('li')].forEach((li, i) => {
        const s = t + 520 + i * 240, lamp = li.querySelector('i')
        sc.from(lamp, { o: 0, s: 0 }, { at: s, dur: 420, ease: E.back })
        sc.fx(lamp, [{ boxShadow: '0 0 16px 2px #ffffffcc', offset: 0 }], { at: s + 250, dur: 700 })
        sc.from(li.querySelector('span'), { o: 0, x: -14 }, { at: s + 80, dur: 600 })
      })
    })
    sc.words('.thanks', { at: 4900 })
    sc.type('.repo', { at: 5500, cps: 55 })
    sc.from('.disclaimer', { o: 0, y: 14 }, { at: 6000, dur: 800 })
  }

  // ------------------------------------------------------------------ boot-time additions to the markup
  function prep() {
    document.querySelectorAll('.slide h1, .tagline, .thanks').forEach(splitWords)
    const wordmark = document.querySelector('.wordmark')
    if (wordmark) splitChars(wordmark, 'ch')
    // self-test rows: the bare label becomes a span so it can be typed on its own
    document.querySelectorAll('.selftest li').forEach(li => {
      for (const n of [...li.childNodes]) if (n.nodeType === 3 && n.textContent.trim()) { const s = mk('span', 'lbl'); s.textContent = n.textContent; n.replaceWith(s) }
    })
    document.querySelectorAll('.figpanel .figbox').forEach(b => b.append(mk('i', 'scanline')))
    // slide 2: the line that answers the crew
    const msg = document.querySelector('.linkpanel .msg')
    if (msg) { const r = mk('div', 'msg reply'); r.innerHTML = '<b>EARTH ▸</b>ответ получен — спустя 40 минут'; msg.after(r) }
    // slide 3: the screenshot and its focus frame move together, like a camera
    const view = document.querySelector('#shot-solution .view')
    if (view) { const cam = mk('div', 'cam'); cam.append(...view.childNodes); view.append(cam) }
    // slide 4: the cursor leaves the figure panel and lives in slide coordinates
    const bodySlide = document.querySelector('.s-body'), cur = bodySlide && bodySlide.querySelector('.cursor')
    if (cur) {
      const p = pointIn(cur, bodySlide, 0, 0)
      bodySlide.append(cur)
      cur.style.cssText = `left:0;top:0;translate:${p.x.toFixed(1)}px ${p.y.toFixed(1)}px`
      bodySlide.append(mk('i', 'click-ring'))
    }
    // slide 6: line and dots sit behind a reveal window, a pen marks the front
    const chart = document.getElementById('pulse-chart')
    if (chart) {
      const defs = smk('defs'), clip = smk('clipPath', { id: 'pc-reveal' }), rect = smk('rect', { x: 0, y: -20, width: 1300, height: 560 })
      clip.append(rect)
      defs.append(clip)
      chart.prepend(defs)
      const g = smk('g', { class: 'g-reveal', 'clip-path': 'url(#pc-reveal)' })
      chart.querySelectorAll('.line').forEach(p => g.append(p))
      const dots = chart.querySelector('.g-dots')
      if (dots) g.append(dots)
      chart.append(g)
      const pen = smk('circle', { class: 'pen', r: 5.5, cx: -20, cy: -20 })
      chart.append(pen)
      chart._motion = { rect, pen, pts: [...chart.querySelectorAll('.dot')].map(c => [+c.getAttribute('cx'), +c.getAttribute('cy')]) }
    }
    // slide 7: a bright trace per wire, packets, and the second face of the web client
    const arch = document.querySelector('.arch')
    if (arch) {
      const wiresSvg = arch.querySelector('.wires')
      ;[...wiresSvg.querySelectorAll('path')].forEach(p => { const h = p.cloneNode(false); h.setAttribute('class', 'wire-hot'); h.setAttribute('pathLength', 1); wiresSvg.append(h) })
      WIRES.forEach(() => arch.append(mk('i', 'pk')))
      const thumb = arch.querySelector('.ui-box .thumb')
      if (thumb) { const alt = new Image(); alt.src = 'assets/screens/web-crew-overview.webp'; alt.className = 'alt'; alt.alt = ''; thumb.append(alt) }
    }
  }

  // ------------------------------------------------------------------ slide changes
  let enabled = !new URLSearchParams(location.search).has('static')
  let current = null
  const sweep = mk('i')
  sweep.id = 'sweep'
  stage.append(sweep)

  function leave(slide, dir) {
    const sc = slide._scene
    if (!enabled) { if (sc) { sc.kill(); slide._scene = null } return }
    if (sc) sc.freeze()
    slide.classList.add('is-leaving')
    const a = slide.animate([{ opacity: 1, translate: '0 0' }, { opacity: 0, translate: `${-44 * dir}px 0` }], { duration: 300, easing: E.in, fill: 'forwards' })
    slide._exit = a
    a.finished.then(() => {
      slide.classList.remove('is-leaving')
      a.cancel()
      if (slide._exit === a) slide._exit = null
      if (sc && slide._scene === sc) { sc.kill(); slide._scene = null }
    }, () => { /* cancelled: the slide was entered again */ })
  }
  function enter(slide, dir, first) {
    if (slide._exit) { const a = slide._exit; slide._exit = null; try { a.cancel() } catch { /* done */ } slide.classList.remove('is-leaving') }
    if (slide._scene) { slide._scene.kill(); slide._scene = null }
    const prev = current
    current = slide
    if (prev && prev !== slide) leave(prev, dir)
    if (!enabled) return
    const key = [...slide.classList].find(c => scenes[c])
    const sc = new Scene(slide, first ? 260 : 170)
    slide._scene = sc
    try { if (key) scenes[key](sc) } catch (err) { console.error('scene failed:', key, err); sc.kill(); slide._scene = null; return }
    sc.t0 = 0
    if (!first) sweep.animate([{ translate: `${dir > 0 ? -20 : 1940}px 0`, opacity: 0 }, { opacity: 1, offset: .12 }, { opacity: 1, offset: .85 }, { translate: `${dir > 0 ? 1940 : -20}px 0`, opacity: 0 }], { duration: 720, easing: E.inout })
  }
  const replay = () => { if (current) { const s = current; current = null; enter(s, 1, true) } }

  document.addEventListener('deck:enter', e => enter(e.target, (e.detail && e.detail.dir) || 1, false))
  addEventListener('keydown', e => {
    if (e.ctrlKey || e.metaKey || e.altKey) return
    if (e.key === '0') replay()
    else if (e.key === 'm' || e.key === 'M' || e.key === 'ь' || e.key === 'Ь') {
      enabled = !enabled
      body.classList.toggle('motion-on', enabled)
      if (enabled) replay()
      else D.slides.forEach(s => { if (s._scene) { s._scene.kill(); s._scene = null } })
    }
  })

  // ------------------------------------------------------------------ boot
  try { prep() } catch (err) { console.error('motion prep failed:', err) }
  body.classList.toggle('motion-on', enabled)
  let started = false
  const start = () => {
    if (started) return
    started = true
    body.classList.remove('booting')
    if (!enabled) return
    // the instrument powers on once: bars slide in, the grid comes up
    const top = stage.querySelector('.topbar'), bottom = stage.querySelector('.statusbar'), grid = stage.querySelector('.grid-bg')
    if (top) top.animate([{ translate: '0 -64px' }, { translate: '0 0' }], { duration: 800, easing: E.out })
    if (bottom) bottom.animate([{ translate: '0 40px' }, { translate: '0 0' }], { duration: 800, easing: E.out })
    if (grid) grid.animate([{ opacity: 0, offset: 0 }], { duration: 1800, easing: E.lin })
    enter(D.slides[D.index], 1, true)
  }
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(start)
  setTimeout(start, 1800)
  window.Motion = { replay, get enabled() { return enabled } }
})()
