rofi_mode::export_mode!(Mode);

struct Mode {
    entries: Vec<String>,
}

impl rofi_mode::Mode for Mode {
    const NAME: &'static str = "plugin-example-basic\0";
    const DISPLAY_NAME: &'static str = "A basic Rofi plugin\0";

    fn init() -> Result<Self, ()> {
        Ok(Self {
            entries: Vec::new(),
        })
    }

    fn entries(&self) -> usize {
        self.entries.len()
    }

    fn entry_style(&self, _line: usize) -> rofi_mode::Style {
        rofi_mode::Style::NORMAL
    }

    fn entry(&self, line: usize) -> (rofi_mode::Style, rofi_mode::Attributes, rofi_mode::String) {
        (
            self.entry_style(line),
            rofi_mode::Attributes::new(),
            (&*self.entries[line]).into(),
        )
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
        selected_line: usize,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel => return rofi_mode::Action::Exit,
            rofi_mode::Event::Ok { alt: _ } => {
                println!("Selected option {:?}", self.entries[selected_line]);
                return rofi_mode::Action::Exit;
            }
            rofi_mode::Event::CustomInput { alt: _ } => {
                self.entries.push(input.into());
                input.clear();
            }
            rofi_mode::Event::DeleteEntry => {
                self.entries.remove(selected_line);
            }
            rofi_mode::Event::Complete => {
                input.clear();
                input.push_str(&self.entries[selected_line]);
            }
            rofi_mode::Event::CustomCommand(_) => {}
        }
        rofi_mode::Action::Reload
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        matcher.matches(&*self.entries[line])
    }
}
