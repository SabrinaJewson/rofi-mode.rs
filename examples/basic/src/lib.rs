rofi_mode::export_mode!(Mode<'_>);

struct Mode<'rofi> {
    api: rofi_mode::Api<'rofi>,
    entries: Vec<String>,
}

impl<'rofi> rofi_mode::Mode<'rofi> for Mode<'rofi> {
    const NAME: &'static str = "plugin-example-basic\0";

    fn init(mut api: rofi_mode::Api<'rofi>) -> Result<Self, ()> {
        api.set_display_name("A basic Rofi plugin");
        Ok(Self {
            api,
            entries: Vec::new(),
        })
    }

    fn entries(&mut self) -> usize {
        self.entries.len()
    }

    fn entry_content(&self, line: usize) -> rofi_mode::String {
        (&self.entries[line]).into()
    }

    fn entry_icon(&mut self, _line: usize, height: u32) -> Option<rofi_mode::cairo::Surface> {
        self.api
            .query_icon("computer", height)
            .wait(&mut self.api)
            .ok()
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { selected: _ } => return rofi_mode::Action::Exit,
            rofi_mode::Event::Ok {
                alt: false,
                selected,
            } => {
                println!("Selected option {:?}", self.entries[selected]);
                return rofi_mode::Action::Exit;
            }
            rofi_mode::Event::Ok {
                alt: true,
                selected,
            } => {
                self.api.set_display_name(&*self.entries[selected]);
            }
            rofi_mode::Event::CustomInput {
                alt: false,
                selected: _,
            } => {
                self.entries.push(input.into());
                input.clear();
            }
            rofi_mode::Event::CustomInput {
                alt: true,
                selected: _,
            } => {
                self.api.replace_display_name(mem::take(input));
            }
            rofi_mode::Event::DeleteEntry { selected } => {
                self.entries.remove(selected);
            }
            rofi_mode::Event::Complete {
                selected: Some(selected),
            } => {
                input.clear();
                input.push_str(&self.entries[selected]);
            }
            rofi_mode::Event::Complete { .. } | rofi_mode::Event::CustomCommand { .. } => {}
        }
        rofi_mode::Action::Reload
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        matcher.matches(&self.entries[line])
    }

    fn message(&mut self) -> rofi_mode::String {
        let entries = self.entries.len();
        if entries == 1 {
            "1 entry registered".into()
        } else {
            rofi_mode::format!("{entries} entries registered")
        }
    }
}

use std::mem;
