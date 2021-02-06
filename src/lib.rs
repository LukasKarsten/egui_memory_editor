use std::ops::{Range, RangeInclusive};

use egui::{Align, Color32, CtxRef, Label, Layout, Pos2, Rect, TextStyle, Ui, Vec2, Window};
use num::Integer;

use crate::egui_utilities::*;
use crate::list_clipper::ScrollAreaClipper;
use crate::option_data::MemoryEditorOptions;

mod option_data;
mod list_clipper;
mod egui_utilities;

/// Reads a value present at the provided address in the object `T`.
///
/// # Arguments
///
/// - `&mut T`: the object on which the read should be performed.
/// - `usize`: The address of the read.
type ReadFunction<T> = Box<dyn FnMut(&mut T, usize) -> u8>;
/// Writes the changes the user made to the `T` object.
///
/// # Arguments
///
/// - `&mut T`: the object whose state is to be updated.
/// - `usize`: The address of the intended write.
/// - `u8`: The value set by the user for the provided address.
type WriteFunction<T> = Box<dyn FnMut(&mut T, usize, u8)>;

pub struct MemoryEditor<T> {
    /// The name of the `egui` window, can be left blank.
    window_name: String,
    /// The function used for getting the values out of the provided type `T` and displaying them.
    read_function: Option<ReadFunction<T>>,
    /// The function used when attempts are made to change values within the GUI.
    write_function: Option<WriteFunction<T>>,
    /// The range of possible values to be displayed, the GUI will start at the lower bound and go up to the upper bound.
    address_space: Range<usize>,
    /// The amount of characters used to indicate the current address in the sidebar of the UI.
    /// Is derived from the provided `address_space.end`
    address_characters: usize,
    /// When `true` will disallow any edits, ensuring the `write_function` will never be called.
    /// The latter therefore doesn't need to be set.
    read_only: bool,
    /// A collection of options relevant for the `MemoryEditor` window.
    /// Can optionally be serialized/deserialized with `serde`
    pub options: MemoryEditorOptions,
}


impl<T> MemoryEditor<T> {
    pub fn new(text: impl Into<String>) -> Self {
        MemoryEditor {
            window_name: text.into(),
            read_function: None,
            write_function: None,
            address_space: (0..u16::MAX as usize),
            address_characters: 4,
            read_only: false,
            options: Default::default(),
        }
    }

    pub fn ui(&mut self, ctx: &CtxRef, memory: &mut T) {
        assert!(self.read_function.is_some(), "The read function needs to be set before one can run the editor!");
        assert!(self.write_function.is_some() || self.read_only, "The write function needs to be set if not in read only mode!");

        let mut is_open = self.options.is_open;

        Window::new(self.window_name.clone())
            .open(&mut is_open)
            .scroll(false)
            .default_height(300.)
            .resizable(true)
            .show(ctx, |ui| {
                self.draw_viewer_contents(ui, memory);
            });

        self.options.is_open = is_open;
    }

    fn draw_viewer_contents(&mut self, ui: &mut Ui, memory: &mut T) {
        let Self {
            options,
            read_function,
            address_space,
            address_characters,
            ..
        } = self;

        let MemoryEditorOptions {
            show_options,
            data_preview_options,
            show_ascii_sidebar,
            grey_out_zeros,
            column_count,
            ..
        } = options;

        let mut read_function = read_function.as_mut().unwrap();

        ui.text_edit_singleline(&mut "Hey".to_string());
        ui.button("Hello World");

        egui::ScrollArea::auto_sized().show(ui, |ui| {
            println!("Beginning Scroll: {:?}", get_current_scroll(ui));
            let max_lines = address_space.end.div_ceil(column_count);
            let mut list_clipper = ScrollAreaClipper::new(max_lines, get_label_line_height(ui, TextStyle::Monospace));
            list_clipper.begin(ui);
            let line_ranges = list_clipper.get_current_line_range(ui);
            println!("Range: {:?}", line_ranges);
            println!("Scroll: {:?}", get_current_scroll(ui));
            egui::Grid::new("mem_edit_grid")
                .striped(true)
                .spacing(Vec2::new(15., ui.style().spacing.item_spacing.y))
                .show(ui, |ui| {
                    ui.style_mut().body_text_style = TextStyle::Monospace;
                    ui.style_mut().spacing.item_spacing.x = 3.0;

                    for start_row in line_ranges {
                        let start_address = start_row * *column_count;
                        ui.add(Label::new(format!("0x{:01$X}", start_address, address_characters))
                            .text_color(Color32::from_rgb(120, 0, 120))
                            .heading());

                        for c in 0..column_count.div_ceil(&8) {
                            ui.columns((*column_count - 8 * c).min(8), |columns| {
                                let start_address = start_address + 8 * c;
                                for (i, column) in columns.iter_mut().enumerate() {
                                    let memory_address = (start_address + i);
                                    if !address_space.contains(&memory_address) {
                                        break;
                                    }

                                    let mem_val: u8 = read_function(memory, memory_address);
                                    column.add(Label::new(format!("{:02X}", mem_val)).heading());
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
            list_clipper.finish(ui);
        });
    }

    /// Set the function used to read from the provided object `T`.
    ///
    /// This function will always be necessary, and the editor will `panic` if it's not available.
    pub fn set_read_function(mut self, read_function: ReadFunction<T>) -> Self {
        self.read_function = Some(read_function);
        self
    }

    /// Set the function used to write to the provided object `T`.
    ///
    /// This function is only necessary if `read_only` is `false`.
    pub fn set_write_function(mut self, write_function: WriteFunction<T>) -> Self {
        self.write_function = Some(write_function);
        self
    }

    /// Set the range of addresses that the UI will display, and subsequently query for using the `read_function`
    pub fn set_address_space(mut self, address_range: Range<usize>) -> Self {
        self.address_space = address_range;
        // This is incredibly janky, but I couldn't find a better way atm. If there is one, please mention it!
        self.address_characters = format!("{:X}", self.address_space.end).chars().count();
        self
    }

    /// If set to `true` the UI will not allow any manual memory edits, and thus the `write_function` will never be called
    /// (and therefore doesn't need to be set).
    pub fn set_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

