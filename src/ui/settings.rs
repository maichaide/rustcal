use crate::app::RustCalApp;
use crate::db::Database;
use crate::models::ThemeConfig;
use crate::sync::caldav::SyncEngine;
use crate::sync::ical;
use crate::ui::{color_to_hex, parse_hex_color};
use eframe::egui::{self, Color32, RichText};

pub fn render_sync_and_settings(app: &mut RustCalApp, ui: &mut egui::Ui) {
    let fg_color = parse_hex_color(&app.theme.fg_color);
    let is_importing = app.is_importing();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading(RichText::new("🎨 外观与颜色自定义（前景字体颜色 & 背景颜色）").color(fg_color));
        ui.add_space(4.0);

        let mut theme_changed = false;
        egui::Grid::new("theme_settings_grid")
            .num_columns(2)
            .spacing([14.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new("全局前景字体颜色:").color(fg_color));
                ui.horizontal(|ui| {
                    let mut c = parse_hex_color(&app.theme.fg_color);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        app.theme.fg_color = color_to_hex(c);
                        theme_changed = true;
                    }
                    if ui.text_edit_singleline(&mut app.theme.fg_color).changed() {
                        theme_changed = true;
                    }
                });
                ui.end_row();

                ui.label(RichText::new("全局窗口背景颜色:").color(fg_color));
                ui.horizontal(|ui| {
                    let mut c = parse_hex_color(&app.theme.bg_color);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        app.theme.bg_color = color_to_hex(c);
                        theme_changed = true;
                    }
                    if ui.text_edit_singleline(&mut app.theme.bg_color).changed() {
                        theme_changed = true;
                    }
                });
                ui.end_row();

                ui.label(RichText::new("月视图日期字体颜色 (默认黑色):").color(fg_color));
                ui.horizontal(|ui| {
                    let mut c = parse_hex_color(&app.theme.month_date_fg_color);
                    if ui.color_edit_button_srgba(&mut c).changed() {
                        app.theme.month_date_fg_color = color_to_hex(c);
                        theme_changed = true;
                    }
                    if ui
                        .text_edit_singleline(&mut app.theme.month_date_fg_color)
                        .changed()
                    {
                        theme_changed = true;
                    }
                });
                ui.end_row();

                ui.label(RichText::new("月视图日期背景模式:").color(fg_color));
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut app.theme.month_cell_transparent, "默认无底色 (透明)")
                        .changed()
                    {
                        theme_changed = true;
                    }
                    if !app.theme.month_cell_transparent {
                        let mut c = parse_hex_color(&app.theme.month_cell_bg_color);
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            app.theme.month_cell_bg_color = color_to_hex(c);
                            theme_changed = true;
                        }
                        if ui
                            .text_edit_singleline(&mut app.theme.month_cell_bg_color)
                            .changed()
                        {
                            theme_changed = true;
                        }
                    }
                });
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui.button("💾 保存颜色配置").clicked() || theme_changed {
                app.save_theme();
            }
            if ui.button("↩ 恢复默认 (黑字无底色)").clicked() {
                app.theme = ThemeConfig::default();
                app.save_theme();
                app.status_message = "✅ 已恢复默认主题（黑色字体、月视图日期无底色）".into();
            }
        });

        ui.separator();
        ui.heading(RichText::new("🔄 数据导入 / 导出与全量备份 (异步高性能导入 + 进度条)").color(fg_color));
        ui.label(
            RichText::new(format!(
                "本地 SQLite 数据库路径: {}",
                Database::default_db_path().display()
            ))
            .color(fg_color),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let import_btn = ui.add_enabled(
                !is_importing,
                egui::Button::new("📥 导入 .ics (支持数千条高速批量导入 + 中文/多编码)"),
            );
            if import_btn.clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("iCalendar", &["ics"])
                    .pick_file()
                {
                    app.start_async_ics_import(path, ui.ctx().clone());
                }
            }

            if ui.button("📤 导出为 .ics (UTF-8 标准)").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("rustcal_export.ics")
                    .save_file()
                {
                    let content = ical::export_to_ics(&app.events, &app.todos);
                    match std::fs::write(&path, content) {
                        Ok(_) => app.status_message = format!("✅ 已导出 ICS 至 {}", path.display()),
                        Err(e) => app.status_message = format!("❌ 导出失败: {}", e),
                    }
                }
            }

		if ui.button("📤 导出为 .ics (UTF-8 标准)").clicked() {
			if let Some(path) = rfd::FileDialog::new()
				.set_file_name("rustcal_export.ics")
				.save_file()
			{
				let content = ical::export_to_ics(&app.events, &app.todos);
				// 【关键修复】：显式以 UTF-8 字节流写入文件，避免受 Windows 本地 ANSI/GBK 编码影响
				match std::fs::write(&path, content.as_bytes()) {
					Ok(_) => app.status_message = format!("✅ 已成功导出 UTF-8 ICS 至 {}", path.display()),
					Err(e) => app.status_message = format!("❌ 导出失败: {}", e),
				}
			}
		}
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("💾 导出完整 JSON 备份").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("rustcal_backup.json")
                    .save_file()
                {
                    match ical::export_to_json(&app.calendars, &app.events, &app.todos) {
                        Ok(json) => match std::fs::write(&path, json) {
                            Ok(_) => {
                                app.status_message =
                                    format!("✅ JSON 完整备份已保存至 {}", path.display())
                            }
                            Err(e) => app.status_message = format!("❌ 保存失败: {}", e),
                        },
                        Err(e) => app.status_message = format!("❌ 序列化失败: {}", e),
                    }
                }
            }

            let restore_btn = ui.add_enabled(
                !is_importing,
                egui::Button::new("♻ 从 JSON 备份恢复"),
            );
            if restore_btn.clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    app.start_async_json_import(path, ui.ctx().clone());
                }
            }
        });

        ui.separator();
        ui.heading(RichText::new("☁ 云端日历同步与协作 (CalDAV / Google / Outlook / WebDAV)").color(fg_color));

        egui::Grid::new("sync_account_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(RichText::new("同步协议 / 生态:").color(fg_color));
                egui::ComboBox::from_id_salt("sync_provider_combo")
                    .selected_text(&app.sync_config.provider)
                    .show_ui(ui, |ui| {
                        for p in ["CalDAV", "Google Calendar", "Microsoft Graph", "WebDAV"] {
                            ui.selectable_value(&mut app.sync_config.provider, p.into(), p);
                        }
                    });
                ui.end_row();

                ui.label(RichText::new("服务器 / Endpoint URL:").color(fg_color));
                ui.text_edit_singleline(&mut app.sync_config.server_url);
                ui.end_row();

                ui.label(RichText::new("账号 / Client ID:").color(fg_color));
                ui.text_edit_singleline(&mut app.sync_config.username);
                ui.end_row();

                ui.label(RichText::new("密码 / OAuth Token:").color(fg_color));
                ui.add(
                    egui::TextEdit::singleline(&mut app.sync_config.token_or_password)
                        .password(true),
                );
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui.button("🔌 测试连接").clicked() {
                match SyncEngine::test_connection(&app.sync_config) {
                    Ok(msg) => app.status_message = msg,
                    Err(e) => app.status_message = format!("❌ {}", e),
                }
            }
            if ui.button("🔄 立即同步所有已启用日历").clicked() {
                let mut logs = Vec::new();
                for cal in &app.calendars {
                    if cal.is_visible {
                        if let Ok(m) = SyncEngine::sync_calendar(cal) {
                            logs.push(m);
                        }
                    }
                }
                app.status_message = logs.join(" | ");
            }
        });

        ui.separator();
        ui.heading(RichText::new("⌨ 键盘快捷键指南 (Calcurse 效率流)").color(fg_color));
        ui.label(
            RichText::new("• 1 / 2 / 3   : 切换 日历 / 任务 / 设置与同步")
                .monospace()
                .color(fg_color),
        );
        ui.label(
            RichText::new("• T           : 迅速跳转回今天")
                .monospace()
                .color(fg_color),
        );
        ui.label(
            RichText::new("• N           : 新建日程 (Appointment)")
                .monospace()
                .color(fg_color),
        );
        ui.label(
            RichText::new("• Shift + N   : 新建任务 (Todo)")
                .monospace()
                .color(fg_color),
        );
        ui.label(
            RichText::new("• /           : 聚焦全局搜索框")
                .monospace()
                .color(fg_color),
        );

        if !app.status_message.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new(&app.status_message)
                    .strong()
                    .color(Color32::from_rgb(5, 150, 105)),
            );
        }
    });
}
