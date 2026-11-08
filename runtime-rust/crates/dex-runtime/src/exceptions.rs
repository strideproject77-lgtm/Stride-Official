//! DEX exception handling, catch regions, propagation, and stack traces.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrameInfo {
    pub class_descriptor: String,
    pub method_name: String,
    pub prototype: String,
    pub code_unit_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStackTrace {
    pub frames: Vec<StackFrameInfo>,
}

impl RuntimeStackTrace {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    pub fn push_frame(&mut self, class_descriptor: &str, method_name: &str, prototype: &str, offset: usize) {
        self.frames.push(StackFrameInfo {
            class_descriptor: class_descriptor.to_owned(),
            method_name: method_name.to_owned(),
            prototype: prototype.to_owned(),
            code_unit_offset: offset,
        });
    }

    pub fn format_trace(&self) -> String {
        let mut out = String::new();
        for frame in &self.frames {
            out.push_str(&format!(
                "  at {}.{}{} (offset {})\n",
                frame.class_descriptor, frame.method_name, frame.prototype, frame.code_unit_offset
            ));
        }
        out
    }
}

/// Catch handler entry: target address and optional exception type descriptor.
/// `None` indicates a catch-all handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchHandler {
    pub catch_type: Option<String>,
    pub target_pc: usize,
}

/// A protected try block spanning `[start_pc, end_pc)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryBlock {
    pub start_pc: usize,
    pub end_pc: usize,
    pub handlers: Vec<CatchHandler>,
}

impl TryBlock {
    pub fn covers(&self, pc: usize) -> bool {
        pc >= self.start_pc && pc < self.end_pc
    }

    pub fn find_handler<F>(&self, exception_type: &str, is_assignable: F) -> Option<usize>
    where
        F: Fn(&str, &str) -> bool,
    {
        for handler in &self.handlers {
            match &handler.catch_type {
                None => return Some(handler.target_pc), // Catch-all
                Some(catch_type) => {
                    if is_assignable(catch_type, exception_type) {
                        return Some(handler.target_pc);
                    }
                }
            }
        }
        None
    }
}

/// Thrown managed exception details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrownException {
    pub exception_object: u32,
    pub class_descriptor: String,
    pub message: Option<String>,
    pub stack_trace: RuntimeStackTrace,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_block_matches_typed_and_catch_all_handlers() {
        let block = TryBlock {
            start_pc: 2,
            end_pc: 10,
            handlers: vec![
                CatchHandler {
                    catch_type: Some("Ljava/lang/NullPointerException;".into()),
                    target_pc: 20,
                },
                CatchHandler {
                    catch_type: None, // Catch-all
                    target_pc: 30,
                },
            ],
        };

        assert!(!block.covers(1));
        assert!(block.covers(5));
        assert!(!block.covers(10));

        let pc_typed = block.find_handler("Ljava/lang/NullPointerException;", |t, s| t == s);
        assert_eq!(pc_typed, Some(20));

        let pc_all = block.find_handler("Ljava/lang/IOException;", |t, s| t == s);
        assert_eq!(pc_all, Some(30));
    }
}
