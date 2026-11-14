//! Ground console, laid out like NASA Open MCT: object tree on the left, stacked telemetry plots with a
//! time conductor and an event log in the middle, the selected crew member (body + uplink) on the right.

use super::*;

/// One row of the object tree. Returns the click response.
fn tree_row(ui: &mut Ui, indent: f32, label: &str, right: &str, dot: Color32, selected: bool, strong: bool, chev: Option<bool>, mark: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let p = ui.painter_at(rect);
    if selected {
        p.rect_filled(rect, CornerRadius::ZERO, alpha(ACCENT, 0.22));
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::ZERO, SURFACE2);
    }
    if mark {
        p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(2.0, 16.0)), CornerRadius::ZERO, ACCENT);
    }
    let mut x = rect.left() + 6.0 + indent;
    if let Some(open) = chev {
        let c = pos2(x + 4.0, rect.center().y);
        let pts = if open { vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 3.0)] } else { vec![c + vec2(-2.0, -3.5), c + vec2(3.0, 0.0), c + vec2(-2.0, 3.5)] };
        p.add(egui::Shape::convex_polygon(pts, MUT, Stroke::NONE));
    }
    x += 14.0;
    p.rect_filled(Rect::from_center_size(pos2(x + 3.0, rect.center().y), vec2(6.0, 6.0)), CornerRadius::ZERO, dot);
    let font = if strong { display(13.0) } else { FontId::proportional(12.5) };
    p.text(pos2(x + 14.0, rect.center().y), Align2::LEFT_CENTER, label, font, TXT);
    p.text(pos2(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, right, mono(11.5), MUT);
    resp
}

impl<B: Backend> GuiApp<B> {
    /// Default telemetry channels: the selected crew member's most severe findings.
    pub(super) fn sync_watch(&mut self) {
        let sel = self.b.selected_crew();
        if self.watch_crew != sel {
            self.watch_crew = sel;
            self.watch_touched = false;
        }
        if !self.watch_touched {
            let mut v: Vec<String> = self.issues().into_iter().map(|(i, _, _)| i.id).take(3).collect();
            if v.is_empty() {
                v = vec!["hr".into(), "spo2".into()];
            }
            self.watch = v;
        }
    }

    pub(super) fn tree_panel(&mut self, ctx: &egui::Context) {
        let crew = self.b.crew();
        let sel = self.b.selected_crew();
        let mut pick: Option<String> = None;
        let mut flip_node: Option<String> = None;
        let mut flip_watch: Option<String> = None;
        let mut open_metric: Option<String> = None;
        egui::SidePanel::left("tree").exact_width(292.0).resizable(false).frame(Frame::new().fill(BG).inner_margin(Margin::same(12))).show(ctx, |ui| {
            caption(ui, "Objects");
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for c in &crew {
                    let selected = c.id == sel;
                    let score = c.score.map(|s| s.to_string()).unwrap_or("-".into());
                    if tree_row(ui, 0.0, &c.name, &score, status_color(c.status), selected, true, Some(selected), false).clicked() {
                        pick = Some(c.id.clone());
                    }
                    if !selected {
                        continue;
                    }
                    for s in &self.systems {
                        let st = self.b.system_status(&s.id);
                        let key = format!("sys:{}", s.id);
                        let open = (st >= Status::Watch) != self.flipped.contains(&key);
                        if tree_row(ui, 14.0, &short_name(&s.name), "", status_color(st), false, false, Some(open), false).clicked() {
                            flip_node = Some(key);
                        }
                        if !open {
                            continue;
                        }
                        for i in self.b.indicators(&s.id) {
                            let ist = self.b.indicator_status(&i.id);
                            let val = self.b.latest(&i.id).map(|r| format!("{} {}", fmt_num(r.value), i.unit)).unwrap_or("-".into());
                            let watched = self.watch.contains(&i.id);
                            let r = tree_row(ui, 30.0, &i.name, &val, status_color(ist), false, false, None, watched);
                            if r.double_clicked() {
                                open_metric = Some(i.id.clone());
                            } else if r.clicked() {
                                flip_watch = Some(i.id.clone());
                            }
                        }
                    }
                    ui.add_space(4.0);
                }
            });
        });
        if let Some(id) = pick {
            self.b.select_crew(&id);
            self.reset_inputs();
            self.nav = Nav::Overview;
        }
        if let Some(k) = flip_node {
            if !self.flipped.remove(&k) {
                self.flipped.insert(k);
            }
        }
        if let Some(id) = flip_watch {
            self.watch_touched = true;
            if let Some(p) = self.watch.iter().position(|w| *w == id) {
                self.watch.remove(p);
            } else {
                self.watch.push(id);
            }
        }
        if let Some(id) = open_metric {
            self.go_metric(&id);
        }
    }

    /// Selected crew member: body figure and the uplink composer.
    pub(super) fn right_panel(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        let animate = !self.opts.lite;
        let o = self.b.overview();
        let name = self.crew_name();
        let msgs = self.b.messages();
        let mut compose = std::mem::take(&mut self.compose);
        let mut send = false;
        egui::SidePanel::right("subject").exact_width(340.0).resizable(false).frame(Frame::new().fill(BG).inner_margin(Margin::same(12))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).font(display(15.0)).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut m = self.mode;
                    if segmented(ui, &mut m, &[(figure::Mode::XRay, "X-ray"), (figure::Mode::Skeleton, "Skeleton"), (figure::Mode::Organs, "Organs")]) {
                        self.mode = m;
                    }
                });
            });
            ui.add_space(4.0);
            let sel = self.selected_sys();
            let h = (ui.available_height() - 262.0).max(220.0);
            if let Some(c) = self.fig.show(ui, vec2(ui.available_width(), h), &|s| self.b.system_status(s), &sel, self.mode, time, animate) {
                self.go_system(&c);
            }
            ui.add_space(8.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                caption(ui, &format!("Uplink message to {}", name));
                ui.horizontal_wrapped(|ui| {
                    for t in ["Repeat the measurement in 1 hour.", "Check cabin CO2 and ventilation.", "Rest, hydrate, re-test later today.", "Schedule a private medical conference."] {
                        if ui.small_button(t).clicked() {
                            compose = t.to_string();
                        }
                    }
                });
                ui.add(egui::TextEdit::multiline(&mut compose).desired_rows(2).desired_width(f32::INFINITY).hint_text("Guidance for the crew member"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!compose.trim().is_empty(), primary("Send")).clicked() {
                        send = true;
                    }
                    ui.label(RichText::new(format!("one-way delay about {}", rtt_text(o.rtt_s / 2.0))).size(11.0).color(MUT));
                });
                if let Some(m) = msgs.last() {
                    ui.add(egui::Label::new(RichText::new(format!("Sent day {}: {}", m.day, m.text)).size(11.0).color(MUT)).truncate());
                }
            });
        });
        if send {
            self.b.send_message(compose.trim());
            compose.clear();
        }
        self.compose = compose;
    }

    /// Centre pane: crew status strip, time conductor, stacked plots, event log.
    pub(super) fn ground_console(&mut self, ui: &mut Ui) {
        self.sync_watch();
        let crew = self.b.crew();
        let sel = self.b.selected_crew();
        let o = self.b.overview();
        if crew.is_empty() {
            ui.label(RichText::new("This backend reports a single crew member.").color(MUT));
        }
        // crew status strip
        let gap = 8.0;
        let n = crew.len().max(1) as f32;
        let w = (ui.available_width() - gap * (n - 1.0)) / n - 1.0;
        let mut pick: Option<String> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for c in &crew {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 50.0), Sense::click());
                let p = ui.painter_at(rect);
                let selected = c.id == sel;
                p.add(panel_shape(rect, CUT, if resp.hovered() { SURFACE2 } else { SURFACE }, stroke(1.0, if selected { ACCENT } else { LINE })));
                if selected {
                    p.rect_filled(Rect::from_min_size(rect.left_bottom() + vec2(1.0, -3.0), vec2(rect.width() - 2.0, 2.0)), CornerRadius::ZERO, ACCENT);
                }
                let col = status_color(c.status);
                p.rect_filled(Rect::from_center_size(pos2(rect.left() + 14.0, rect.top() + 17.0), vec2(7.0, 7.0)), CornerRadius::ZERO, col);
                p.text(rect.min + vec2(26.0, 17.0), Align2::LEFT_CENTER, &c.name, display(13.0), TXT);
                let stale = !o.link_up || c.last_contact_s > 3600;
                let sub = if stale { format!("STALE {}", age_text(c.last_contact_s)) } else if c.open_alerts == 0 { "no open findings".to_string() } else { format!("{} open", c.open_alerts) };
                p.text(rect.min + vec2(26.0, 35.0), Align2::LEFT_CENTER, sub, FontId::proportional(10.5), if stale { WARN } else if c.open_alerts > 0 { col } else { MUT });
                p.text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, c.score.map(|s| s.to_string()).unwrap_or("-".into()), mono(18.0), TXT);
                if resp.clicked() {
                    pick = Some(c.id.clone());
                }
            }
        });
        if let Some(id) = pick {
            self.b.select_crew(&id);
            self.reset_inputs();
        }
        ui.add_space(12.0);

        // time conductor
        ui.horizontal(|ui| {
            caption(ui, "Telemetry");
            ui.label(RichText::new(format!("mission day {}", o.mission_day)).font(mono(11.0)).color(MUT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut r = self.range;
                if segmented(ui, &mut r, &[(Range::All, "All"), (Range::D14, "14D"), (Range::D7, "7D")]) {
                    self.range = r;
                }
                ui.label(RichText::new("TIME").size(9.5).color(MUT).strong());
            });
        });
        ui.add_space(4.0);
        let window = match self.range {
            Range::D7 => Some(7),
            Range::D14 => Some(14),
            Range::All => None,
        };
        let mut drop: Option<String> = None;
        let mut open: Option<String> = None;
        if self.watch.is_empty() {
            ui.label(RichText::new("No channels selected. Click an indicator in the object tree to plot it.").size(12.0).color(MUT));
        }
        for id in self.watch.clone() {
            let Some(ind) = self.ind(&id) else { continue };
            let hist = self.b.history(&id);
            let th = self.b.thresholds(&id);
            let st = self.b.indicator_status(&id);
            let col = status_color(st);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    led(ui, col);
                    if ui.add(egui::Label::new(RichText::new(&ind.name).font(display(13.0)).strong()).sense(Sense::click())).clicked() {
                        open = Some(id.clone());
                    }
                    ui.label(RichText::new(hist.last().map(|r| format!("{} {}", fmt_num(r.value), ind.unit)).unwrap_or("-".into())).font(mono(13.0)).color(if st == Status::Normal { TXT } else { col }));
                    if st != Status::Normal {
                        pill(ui, &st.name().to_uppercase(), col);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            drop = Some(id.clone());
                        }
                    });
                });
                chart(ui, &hist, th, ind.min, ind.max, o.mission_day, 104.0, window, &ind.unit);
            });
            ui.add_space(6.0);
        }
        if let Some(id) = drop {
            self.watch_touched = true;
            self.watch.retain(|w| *w != id);
        }
        if let Some(id) = open {
            self.go_metric(&id);
        }
        ui.add_space(6.0);

        // event log
        caption(ui, "Event log");
        ui.add_space(4.0);
        let events = self.b.events(60);
        let mut ack: Option<u64> = None;
        let mut open: Option<String> = None;
        // severity, source, value, day and the button take about 460 px; the rest belongs to the event text
        let text_w = (ui.available_width() - 520.0).clamp(220.0, 900.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if events.is_empty() {
                ui.label(RichText::new("No events for this crew member.").size(12.0).color(MUT));
            }
            egui::Grid::new("events").num_columns(6).spacing([14.0, 7.0]).striped(true).show(ui, |ui| {
                for h in ["Severity", "Source", "Event", "Value", "Day", ""] {
                    ui.label(RichText::new(h.to_uppercase()).size(9.5).color(MUT).strong());
                }
                ui.end_row();
                for e in &events {
                    let (sev, c) = match e.kind {
                        EventKind::Finding => (e.severity.name().to_uppercase(), status_color(e.severity)),
                        EventKind::Uplink => ("UPLINK".to_string(), ACCENT),
                        EventKind::Crew => ("CREW".to_string(), ACCENT),
                    };
                    ui.label(RichText::new(sev).size(10.0).color(c).strong());
                    let src = ui.add(egui::Label::new(RichText::new(&e.source).size(12.0)).sense(Sense::click()));
                    if src.clicked() {
                        open = e.indicator.clone();
                    }
                    ui.allocate_ui_with_layout(vec2(text_w, 16.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&e.text).size(11.5).color(MUT)).truncate());
                    });
                    ui.label(RichText::new(e.value.clone().unwrap_or_default()).font(mono(11.5)));
                    ui.label(RichText::new(e.day.to_string()).font(mono(11.5)).color(MUT));
                    match (e.kind, e.alert_id) {
                        (EventKind::Finding, Some(id)) if !e.acked => {
                            if ui.small_button("Acknowledge").clicked() {
                                ack = Some(id);
                            }
                        }
                        (EventKind::Finding, _) => {
                            ui.label(RichText::new("ACK").size(10.0).color(MUT));
                        }
                        _ => {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(a) = ack {
            self.b.ack_alert(a);
        }
        if let Some(i) = open {
            self.go_metric(&i);
        }
    }
}
