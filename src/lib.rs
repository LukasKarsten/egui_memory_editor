//! # Egui Memory Editor
//!
//! Provides a memory editor to be used with `egui`.
//! Primarily intended for emulation development.
//!
//! Look at [`MemoryEditor`] to get started.
use std::collections::BTreeMap;
use std::ops::Range;

use egui::{Align, Context, Label, Layout, RichText, ScrollArea, Sense, TextEdit, Ui, Vec2, Window};

use crate::option_data::{BetweenFrameData, MemoryEditorOptions};

pub mod option_data;
mod option_ui;
mod utilities;

/// A memory address that should be read from/written to.
pub type Address = usize;

/// The main struct for the editor window.
/// This should persist between frames as it keeps track of quite a bit of state.
#[derive(Clone)]
pub struct MemoryEditor {
    /// The name of the `egui` window, can be left blank.
    window_name: String,
    /// The collection of address ranges, the GUI will start at the lower bound and go up to the upper bound.
    ///
    /// Note this *currently* only supports ranges that have a max of `2^(24+log_2(column_count))` due to `ScrollArea` limitations.
    address_ranges: BTreeMap<String, Range<Address>>,
    /// A collection of options relevant for the `MemoryEditor` window.
    /// Can optionally be serialized/deserialized with `serde`
    pub options: MemoryEditorOptions,
    /// Data for layout between frames, rather hacky.
    frame_data: BetweenFrameData,
    /// The visible range of addresses from the last frame.
    visible_range: Range<Address>,
}

impl MemoryEditor {
    /// Create the MemoryEditor, which should be kept in memory between frames.
    ///
    /// The `read_function` should return one `u8` value from the object which you provide in
    /// either the [`Self::window_ui`] or the [`Self::draw_editor_contents`] method.
    ///
    /// ```no_run
    /// # use egui_memory_editor::MemoryEditor;
    /// # let ctx = egui::Context::default();
    /// let mut memory_base = vec![0xFF; 0xFF];
    /// let mut memory_editor = MemoryEditor::new().with_address_range("Memory", 0..0xFF);
    ///
    /// // Show a read-only window
    /// memory_editor.window_ui_read_only(&ctx, &mut memory_base, |mem, addr| mem[addr]);
    /// ```
    pub fn new() -> Self {
        MemoryEditor {
            window_name: "Memory Editor".to_string(),
            address_ranges: BTreeMap::new(),
            options: Default::default(),
            frame_data: Default::default(),
            visible_range: Default::default(),
        }
    }

    /// Returns the visible range of the last frame.
    ///
    /// Can be useful for asynchronous memory querying.
    pub fn visible_range(&self) -> &Range<Address> {
        &self.visible_range
    }

    /// Create a read-only window and render the memory editor contents within.
    ///
    /// If you want to make your own window/container to be used for the editor contents, you can use [`Self::draw_editor_contents`].
    /// If you wish to be able to write to the memory, you can use [`Self::window_ui`].
    pub fn window_ui_read_only<T: ?Sized>(
        &mut self,
        ctx: &Context,
        mem: &mut T,
        read_fn: impl FnMut(&mut T, Address) -> u8,
    ) {
        // This needs to exist due to the fact we want to use generics, and `Option` needs to know the size of its contents.
        type DummyWriteFunction<T> = fn(&mut T, Address, u8);

        self.window_ui_impl(ctx, mem, read_fn, None::<DummyWriteFunction<T>>);
    }

    /// Create a window and render the memory editor contents within.
    ///
    /// If you want to make your own window/container to be used for the editor contents, you can use [`Self::draw_editor_contents`].
    /// If you wish for read-only access to the memory, you can use [`Self::window_ui_read_only`].
    pub fn window_ui<T: ?Sized>(
        &mut self,
        ctx: &Context,
        mem: &mut T,
        read_fn: impl FnMut(&mut T, Address) -> u8,
        write_fn: impl FnMut(&mut T, Address, u8),
    ) {
        self.window_ui_impl(ctx, mem, read_fn, Some(write_fn));
    }

    fn window_ui_impl<T: ?Sized>(
        &mut self,
        ctx: &Context,
        mem: &mut T,
        read_fn: impl FnMut(&mut T, Address) -> u8,
        write_fn: Option<impl FnMut(&mut T, Address, u8)>,
    ) {
        let mut is_open = self.options.is_open;

        Window::new(self.window_name.clone())
            .open(&mut is_open)
            .hscroll(false)
            .vscroll(false)
            .resizable(true)
            .show(ctx, |ui| {
                self.shrink_window_ui(ui);
                self.draw_editor_contents(ui, mem, read_fn, write_fn);
            });

        self.options.is_open = is_open;
    }

    /// Draws the actual memory viewer/editor.
    ///
    /// Can be included in whatever container you want.
    ///
    /// Use [`Self::window_ui`] if you want to have a window with the contents instead.
    ///
    /// If no `write_fn` function is provided, the editor will be read-only.
    pub fn draw_editor_contents<T: ?Sized>(
        &mut self,
        ui: &mut Ui,
        mem: &mut T,
        mut read_fn: impl FnMut(&mut T, Address) -> u8,
        mut write_fn: Option<impl FnMut(&mut T, Address, u8)>,
    ) {
        assert!(
            !self.address_ranges.is_empty(),
            "At least one address range needs to be added to render the contents!"
        );

        self.draw_options_area(ui, mem, &mut read_fn);

        ui.separator();

        let MemoryEditorOptions {
            show_ascii,
            column_count,
            address_text_colour,
            highlight_text_colour,
            selected_address_range,
            memory_editor_address_text_style,
            ..
        } = self.options.clone();

        let line_height = self.get_line_height(ui);
        let address_space = self.address_ranges.get(&selected_address_range).unwrap().clone();
        // This is janky, but can't think of a better way.
        let address_characters = format!("{:X}", address_space.end).chars().count();
        // Memory Editor Part.
        let max_lines = (address_space.len() + column_count - 1) / column_count; // div_ceil

        // For when we're editing memory, don't use the `Response` object as that would screw over downward scrolling.
        self.handle_keyboard_edit_input(&address_space, ui.ctx());

        let mut scroll = ScrollArea::vertical()
            .id_source(selected_address_range)
            .max_height(f32::INFINITY)
            .auto_shrink([false, true]);

        // Scroll to the goto area address line.
        if let Some(addr) = std::mem::take(&mut self.frame_data.goto_address_line) {
            if address_space.contains(&addr) {
                let new_offset = (line_height + ui.spacing().item_spacing.y) * (addr as f32);

                scroll = scroll.vertical_scroll_offset(new_offset);
            }
        }

        scroll.show_rows(ui, line_height, max_lines, |ui, line_range| {
            // Persist the visible range for future queries.
            self.visible_range = line_range.clone();

            egui::Grid::new("mem_edit_grid")
                .striped(true)
                .spacing(Vec2::new(15.0, ui.style().spacing.item_spacing.y))
                .show(ui, |ui| {
                    ui.style_mut().wrap = Some(false);
                    ui.style_mut().spacing.item_spacing.x = 3.0;

                    for start_row in line_range.clone() {
                        let start_address = address_space.start + (start_row * column_count);
                        let line_range = start_address..start_address + column_count;
                        let highlight_in_range = matches!(self.frame_data.selected_highlight_address, Some(address) if line_range.contains(&address));

                        let start_text = RichText::new(format!("0x{:01$X}:", start_address, address_characters))
                            .color(if highlight_in_range { highlight_text_colour } else { address_text_colour })
                            .text_style(memory_editor_address_text_style.clone());

                        ui.label(start_text);

                        self.draw_memory_values(ui, mem, &mut read_fn, &mut write_fn, start_address, &address_space);

                        if show_ascii {
                            self.draw_ascii_sidebar(ui, mem, &mut read_fn, start_address, &address_space);
                        }

                        ui.end_row();
                    }
                });
            // After we've drawn the area we want to resize to we want to save this size for the next frame.
            // In case it has became smaller we'll shrink the window.
            self.frame_data.previous_frame_editor_width = ui.min_rect().width();
        });
    }

    fn draw_memory_values<T: ?Sized>(
        &mut self,
        ui: &mut Ui,
        mem: &mut T,
        read_fn: &mut impl FnMut(&mut T, Address) -> u8,
        write_fn: &mut Option<impl FnMut(&mut T, Address, u8)>,
        start_address: Address,
        address_space: &Range<Address>,
    ) {
        let frame_data = &mut self.frame_data;
        let options = &self.options;
        let mut read_only = frame_data.selected_edit_address.is_none() || write_fn.is_none();

        for grid_column in 0..(options.column_count + 7) / 8 {
            // div_ceil
            let start_address = start_address + 8 * grid_column;
            // We use columns here instead of horizontal_for_text() to keep consistent spacing for non-monospace fonts.
            // When fonts are more customizable (e.g, we can accept a `Font` as a setting instead of `TextStyle`) I'd like
            // to switch to horizontal_for_text() as we can then just assume a decent Monospace font provided by the user.
            ui.columns((options.column_count - 8 * grid_column).min(8), |columns| {
                for (i, column) in columns.iter_mut().enumerate() {
                    let memory_address = start_address + i;

                    if !address_space.contains(&memory_address) {
                        break;
                    }

                    let mem_val: u8 = read_fn(mem, memory_address);

                    let label_text = format!("{:02X}", mem_val);

                    // Memory Value Labels
                    if !read_only
                        && matches!(frame_data.selected_edit_address, Some(address) if address == memory_address)
                    {
                        // For Editing
                        let response = column.with_layout(Layout::right_to_left(), |ui| {
                            ui.add(
                                TextEdit::singleline(&mut frame_data.selected_edit_address_string)
                                    .desired_width(6.0)
                                    .font(options.memory_editor_text_style.clone())
                                    .hint_text(label_text),
                            )
                        });
                        if frame_data.selected_edit_address_request_focus {
                            frame_data.selected_edit_address_request_focus = false;
                            column.memory().request_focus(response.inner.id);
                        }

                        // Filter out any non Hex-Digit, there doesn't seem to be a method in TextEdit for this.
                        frame_data
                            .selected_edit_address_string
                            .retain(|c| c.is_ascii_hexdigit());

                        // Don't want more than 2 digits
                        if frame_data.selected_edit_address_string.chars().count() >= 2 {
                            let next_address = memory_address + 1;
                            let new_value = u8::from_str_radix(&frame_data.selected_edit_address_string[0..2], 16);

                            if let Ok(value) = new_value {
                                if let Some(write_fns) = write_fn.as_mut() {
                                    write_fns(mem, memory_address, value);
                                }
                            }

                            frame_data.set_selected_edit_address(Some(next_address), address_space);
                        } else if !column.ctx().memory().has_focus(response.inner.id) {
                            // We use has_focus() instead of response.inner.lost_focus() due to the latter
                            // having a bug where it doesn't detect it lost focus when you scroll.
                            frame_data.set_selected_edit_address(None, address_space);
                            read_only = true;
                        }
                    } else {
                        // Read-only values.
                        let mut text = RichText::new(label_text).text_style(options.memory_editor_text_style.clone());

                        if options.show_zero_colour && mem_val == 0 {
                            text = text.color(options.zero_colour);
                        } else {
                            text = text.color(column.style().visuals.text_color());
                        };

                        if frame_data.should_highlight(memory_address) {
                            text = text.color(options.highlight_text_colour);
                        }

                        if frame_data.should_subtle_highlight(memory_address, options.data_preview.selected_data_format)
                        {
                            text = text.background_color(column.style().visuals.code_bg_color);
                        }

                        let label = Label::new(text).sense(Sense::click());

                        // This particular layout is necessary to stop the memory values gradually shifting over to the right
                        // Presumably due to some floating point error when using left_to_right()
                        let response = column.with_layout(Layout::right_to_left(), |ui| ui.add(label));
                        // Right click always selects.
                        if response.inner.secondary_clicked() {
                            frame_data.set_highlight_address(memory_address);
                        }
                        // Left click depends on read only mode.
                        if response.inner.clicked() {
                            if write_fn.is_some() {
                                frame_data.set_selected_edit_address(Some(memory_address), address_space);
                            } else {
                                frame_data.set_highlight_address(memory_address);
                            }
                        }
                    }
                }
            });
        }
    }

    fn draw_ascii_sidebar<T: ?Sized>(
        &mut self,
        ui: &mut Ui,
        mem: &mut T,
        read_fn: &mut impl FnMut(&mut T, Address) -> u8,
        start_address: Address,
        address_space: &Range<Address>,
    ) {
        let options = &self.options;
        // Not pretty atm, needs a better method: TODO
        ui.horizontal(|ui| {
            ui.add(egui::Separator::default().vertical().spacing(3.0));
            ui.style_mut().spacing.item_spacing.x = 0.0;
            ui.columns(options.column_count, |columns| {
                for (i, column) in columns.iter_mut().enumerate() {
                    let memory_address = start_address + i;

                    if !address_space.contains(&memory_address) {
                        break;
                    }

                    let mem_val: u8 = read_fn(mem, memory_address);
                    let character = if !(32..128).contains(&mem_val) {
                        '.'
                    } else {
                        mem_val as char
                    };
                    let mut text = RichText::new(character).text_style(options.memory_editor_ascii_text_style.clone());

                    if self.frame_data.should_highlight(memory_address) {
                        text = text
                            .color(self.options.highlight_text_colour)
                            .background_color(column.style().visuals.code_bg_color);
                    }

                    column.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        ui.label(text);
                    });
                }
            });
        });
    }

    /// Return the line height for the current provided `Ui` and selected `TextStyle`s
    fn get_line_height(&self, ui: &mut Ui) -> f32 {
        let address_size = ui.text_style_height(&self.options.memory_editor_address_text_style);
        let body_size = ui.text_style_height(&self.options.memory_editor_text_style);
        let ascii_size = ui.text_style_height(&self.options.memory_editor_ascii_text_style);
        address_size.max(body_size).max(ascii_size)
    }

    /// Shrink the window to the previous frame's memory viewer's width.
    /// This essentially allows us to only have height resize, and have width grow/shrink as appropriate.
    fn shrink_window_ui(&self, ui: &mut Ui) {
        ui.set_max_width(ui.min_rect().width().min(self.frame_data.previous_frame_editor_width));
    }

    /// Check for arrow keys when we're editing a memory value at an address.
    fn handle_keyboard_edit_input(&mut self, address_range: &Range<Address>, ctx: &Context) {
        use egui::Key::*;
        if self.frame_data.selected_edit_address.is_none() {
            return;
        }
        // We know it must exist otherwise this function can't be called, so safe to unwrap.
        let current_address = self.frame_data.selected_edit_address.unwrap();
        let keys = [ArrowLeft, ArrowRight, ArrowDown, ArrowUp];
        let key_pressed = keys.iter().find(|&&k| ctx.input().key_pressed(k));
        if let Some(key) = key_pressed {
            let next_address = match key {
                ArrowDown => current_address + self.options.column_count,
                ArrowLeft => current_address - 1,
                ArrowRight => current_address + 1,
                ArrowUp => {
                    if current_address < self.options.column_count {
                        0
                    } else {
                        current_address - self.options.column_count
                    }
                }
                _ => unreachable!(),
            };

            self.frame_data
                .set_selected_edit_address(Some(next_address), address_range);
            // Follow the edit cursor whilst moving with the arrow keys.
            //self.frame_data.goto_address_line = Some(next_address / self.options.column_count);
        }
    }

    // ** Builder methods **

    /// Set the window title, only relevant if using the `window_ui()` call.
    pub fn with_window_title(mut self, title: impl Into<String>) -> Self {
        self.window_name = title.into();
        self
    }

    /// Add an address range to the range list.
    /// Multiple address ranges can be added, and will be displayed in the UI by a drop-down box if more than one
    /// range was added.
    ///
    /// The first range that is added will be displayed by default when launching the UI.
    ///
    /// The UI will query your set `read_function` with the values within this `Range`
    #[must_use]
    pub fn with_address_range(mut self, range_name: impl Into<String>, address_range: Range<Address>) -> Self {
        self.address_ranges.insert(range_name.into(), address_range);
        self.frame_data.memory_range_combo_box_enabled = self.address_ranges.len() > 1;
        if let Some((name, _)) = self.address_ranges.iter().next() {
            self.options.selected_address_range = name.clone();
        }
        self
    }

    /// Set the memory options, useful if you use the `persistence` feature.
    pub fn with_options(mut self, options: MemoryEditorOptions) -> Self {
        self.options = options;
        self
    }
}

impl Default for MemoryEditor {
    fn default() -> Self {
        MemoryEditor::new()
    }
}
