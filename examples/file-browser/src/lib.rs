rofi_mode::export_mode!(Mode<'_>);

struct Mode<'rofi> {
    api: rofi_mode::Api<'rofi>,
    dir: PathBuf,
    entries: Vec<Entry>,
    home_dir: Option<PathBuf>,
}

impl<'rofi> rofi_mode::Mode<'rofi> for Mode<'rofi> {
    const NAME: &'static str = "plugin-example-file-browser\0";

    fn init(api: rofi_mode::Api<'rofi>) -> Result<Self, ()> {
        let dir = env::current_dir().map_err(drop)?;
        // `home_dir` is only deprecated because of Windows behaviour; on Unix it’s fine
        #[allow(deprecated)]
        let home_dir = env::home_dir();
        let mut this = Self {
            api,
            dir,
            entries: vec![Entry {
                file_name: "..".into(),
                icon_name: None,
                file_type: FileType::Dir,
            }],
            home_dir,
        };
        this.update_entries();
        Ok(this)
    }

    fn entries(&mut self) -> usize {
        self.entries.len()
    }

    fn entry_content(&self, line: usize) -> rofi_mode::String {
        self.entries[line].file_name.to_string_lossy().into()
    }

    fn entry_icon(&mut self, line: usize, height: u32) -> Option<rofi_mode::cairo::Surface> {
        if let Some(icon_name) = &self.entries[line].icon_name {
            self.api
                .query_icon_cstr(icon_name, height)
                .wait(&mut self.api)
        } else {
            None
        }
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { selected: _ } => return rofi_mode::Action::Exit,
            rofi_mode::Event::Ok { alt: _, selected } => {
                let file_name = &self.entries[selected].file_name;
                if file_name == ".." {
                    self.dir.pop();
                } else {
                    self.dir.push(file_name);
                }
                match self.entries[selected].file_type {
                    FileType::Dir => self.update_entries(),
                    FileType::File => {
                        println!("{}", self.dir.display());
                        return rofi_mode::Action::Exit;
                    }
                }
                input.clear();
            }
            rofi_mode::Event::Complete {
                selected: Some(selected),
            } => {
                *input = self.entry_content(selected);
            }
            rofi_mode::Event::Complete { .. }
            | rofi_mode::Event::CustomInput { .. }
            | rofi_mode::Event::CustomCommand { .. }
            | rofi_mode::Event::DeleteEntry { .. } => {}
        }
        rofi_mode::Action::Reload
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        if let Some(s) = self.entries[line].file_name.to_str() {
            matcher.matches(s)
        } else {
            false
        }
    }

    fn message(&mut self) -> rofi_mode::String {
        let entries = self.entries.len();
        if entries == 1 {
            "1 item in this directory".into()
        } else {
            rofi_mode::format!("{entries} items in this directory")
        }
    }
}

impl Mode<'_> {
    fn update_entries(&mut self) {
        let in_home = self
            .home_dir
            .as_deref()
            .and_then(|home| self.dir.strip_prefix(home).ok());
        if let Some(in_home) = in_home {
            self.api
                .set_display_name(format_args!("~/{}", in_home.display()));
        } else {
            self.api.set_display_name(self.dir.display());
        }
        self.entries.truncate(1);
        if let Ok(iter) = fs::read_dir(&self.dir) {
            let iter = iter
                .flatten()
                .flat_map(|entry| Entry::new(entry, &mut self.api));
            self.entries.extend(iter);
        }
    }
}

struct Entry {
    file_name: OsString,
    icon_name: Option<CString>,
    file_type: FileType,
}

impl Entry {
    fn new(entry: std::fs::DirEntry, api: &mut rofi_mode::Api<'_>) -> Option<Self> {
        let mut file_name = entry.file_name();
        let file_type = entry.file_type().ok()?;
        let file_type = match file_type.is_dir() {
            true => {
                file_name.push("/");
                FileType::Dir
            }
            false => FileType::File,
        };
        let icon_name = if api.supports_image(&file_name) {
            let mut res = entry.path().into_os_string().into_vec();
            res.push(0);
            Some(CString::from_vec_with_nul(res).unwrap())
        } else {
            None
        };
        Some(Entry {
            file_name,
            icon_name,
            file_type,
        })
    }
}

enum FileType {
    Dir,
    File,
}

use std::env;
use std::ffi::CString;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt as _;
use std::path::PathBuf;
