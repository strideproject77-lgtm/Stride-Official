//! Class loading, inheritance hierarchy, virtual method overriding, and interface dispatch.

use std::collections::HashMap;
use stride_dex_inspector::classes::{FieldDefinition, MethodDefinition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeClass {
    pub descriptor: String,
    pub superclass_descriptor: Option<String>,
    pub interface_descriptors: Vec<String>,
    pub access_flags: u32,
    pub fields: HashMap<String, FieldDefinition>,
    pub methods: HashMap<(String, String), MethodDefinition>,
    pub is_initialized: bool,
}

impl RuntimeClass {
    pub fn new(descriptor: &str, superclass_descriptor: Option<&str>) -> Self {
        Self {
            descriptor: descriptor.to_string(),
            superclass_descriptor: superclass_descriptor.map(|s| s.to_string()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: false,
        }
    }

    pub fn register_method(&mut self, name: &str, prototype: &str, method_index: u32, is_direct: bool) {
        self.register_method_with_flags(name, prototype, method_index, is_direct, 0x0001);
    }

    /// Register a method while preserving its original DEX access flags.
    pub fn register_method_with_flags(
        &mut self,
        name: &str,
        prototype: &str,
        method_index: u32,
        is_direct: bool,
        access_flags: u32,
    ) {
        self.methods.insert(
            (name.to_string(), prototype.to_string()),
            MethodDefinition {
                method_index,
                name: name.to_string(),
                prototype: prototype.to_string(),
                access_flags,
                code_offset: 0,
                is_direct,
            },
        );
    }

    pub fn is_interface(&self) -> bool {
        (self.access_flags & 0x0200) != 0
    }

    pub fn is_abstract(&self) -> bool {
        (self.access_flags & 0x0400) != 0
    }

    pub fn is_public(&self) -> bool {
        (self.access_flags & 0x0001) != 0
    }
}

/// Manages class inheritance hierarchies, resolving method overrides and interfaces.
#[derive(Debug, Default, Clone)]
pub struct ClassHierarchy {
    classes: HashMap<String, RuntimeClass>,
}

impl ClassHierarchy {
    pub fn new() -> Self {
        let mut hierarchy = Self::default();
        hierarchy.bootstrap_standard_classes();
        hierarchy
    }

    pub fn register_class(&mut self, class: RuntimeClass) {
        self.classes.insert(class.descriptor.clone(), class);
    }

    pub fn get_class(&self, descriptor: &str) -> Option<&RuntimeClass> {
        self.classes.get(descriptor)
    }

    pub fn get_class_mut(&mut self, descriptor: &str) -> Option<&mut RuntimeClass> {
        self.classes.get_mut(descriptor)
    }

    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Check if `sub_type` can be assigned to `super_type` (e.g. `super_type.isAssignableFrom(sub_type)`).
    pub fn is_assignable_from(&self, target_type: &str, source_type: &str) -> bool {
        if target_type == source_type || target_type == "Ljava/lang/Object;" {
            return true;
        }

        let mut current = source_type.to_owned();
        while let Some(class) = self.classes.get(&current) {
            // Check direct match
            if class.descriptor == target_type {
                return true;
            }
            // Check interfaces implemented
            for iface in &class.interface_descriptors {
                if self.is_assignable_from(target_type, iface) {
                    return true;
                }
            }
            // Move up superclass chain
            if let Some(super_desc) = &class.superclass_descriptor {
                current = super_desc.clone();
            } else {
                break;
            }
        }
        false
    }

    /// Resolve virtual method calls against the receiver's runtime class.
    /// Class declarations take precedence; interface defaults use maximally
    /// specific declarations so traversal order cannot select a parent default
    /// over a child override or an abstract redeclaration.
    pub fn resolve_virtual_method(
        &self,
        receiver_class: &str,
        name: &str,
        prototype: &str,
    ) -> Option<(&RuntimeClass, &MethodDefinition)> {
        let key = (name.to_owned(), prototype.to_owned());
        let mut current = receiver_class.to_owned();
        let mut pending_interfaces = Vec::new();

        // A class declaration (including an abstract override) wins over any
        // interface default. Static and private methods are not virtual targets.
        while let Some(class) = self.classes.get(&current) {
            if let Some(method) = class.methods.get(&key) {
                if !method.is_direct && method.access_flags & (0x0008 | 0x0002) == 0 {
                    return Some((class, method));
                }
            }
            pending_interfaces.extend(class.interface_descriptors.iter().cloned());
            if let Some(super_desc) = &class.superclass_descriptor {
                current = super_desc.clone();
            } else {
                break;
            }
        }

        // Collect every reachable interface declaration once. Keep abstract
        // declarations too: a more-specific abstract redeclaration suppresses
        // an inherited parent default.
        let mut visited = std::collections::HashSet::new();
        let mut declarations = Vec::new();
        while let Some(interface) = pending_interfaces.pop() {
            if !visited.insert(interface.clone()) {
                continue;
            }
            if let Some(class) = self.classes.get(&interface) {
                if let Some(method) = class.methods.get(&key) {
                    if !method.is_direct && method.access_flags & (0x0008 | 0x0002) == 0 {
                        declarations.push((class, method));
                    }
                }
                pending_interfaces.extend(class.interface_descriptors.iter().cloned());
            }
        }

        // Discard declarations shadowed by a declaration in a strict
        // subinterface. Multiple unrelated maximal declarations are ambiguous.
        let maximal: Vec<_> = declarations.iter().copied().filter(|(candidate, _)| {
            !declarations.iter().any(|(other, _)| {
                other.descriptor != candidate.descriptor
                    && self.is_assignable_from(&candidate.descriptor, &other.descriptor)
            })
        }).collect();

        if maximal.len() != 1 {
            return None;
        }
        let (owner, method) = maximal[0];
        if method.access_flags & 0x0400 != 0 {
            return None;
        }
        Some((owner, method))
    }

    /// Resolve `invoke-super` call starting from the declaring class's superclass.
    pub fn resolve_super_method(
        &self,
        declaring_class: &str,
        name: &str,
        prototype: &str,
    ) -> Option<(&RuntimeClass, &MethodDefinition)> {
        let class = self.classes.get(declaring_class)?;
        let super_desc = class.superclass_descriptor.as_deref()?;
        self.resolve_virtual_method(super_desc, name, prototype)
    }

    /// Resolve field across class and superclass chain.
    pub fn resolve_field(
        &self,
        class_descriptor: &str,
        field_name: &str,
    ) -> Option<(&RuntimeClass, &FieldDefinition)> {
        let mut current = class_descriptor.to_owned();
        while let Some(class) = self.classes.get(&current) {
            if let Some(field) = class.fields.get(field_name) {
                return Some((class, field));
            }
            if let Some(super_desc) = &class.superclass_descriptor {
                current = super_desc.clone();
            } else {
                break;
            }
        }
        None
    }

    fn bootstrap_standard_classes(&mut self) {
        let object_class = RuntimeClass {
            descriptor: "Ljava/lang/Object;".into(),
            superclass_descriptor: None,
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(object_class);

        let string_class = RuntimeClass {
            descriptor: "Ljava/lang/String;".into(),
            superclass_descriptor: Some("Ljava/lang/Object;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0011,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(string_class);

        let throwable_class = RuntimeClass {
            descriptor: "Ljava/lang/Throwable;".into(),
            superclass_descriptor: Some("Ljava/lang/Object;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(throwable_class);

        let exception_class = RuntimeClass {
            descriptor: "Ljava/lang/Exception;".into(),
            superclass_descriptor: Some("Ljava/lang/Throwable;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(exception_class);

        let runtime_exception_class = RuntimeClass {
            descriptor: "Ljava/lang/RuntimeException;".into(),
            superclass_descriptor: Some("Ljava/lang/Exception;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(runtime_exception_class);

        let npe_class = RuntimeClass {
            descriptor: "Ljava/lang/NullPointerException;".into(),
            superclass_descriptor: Some("Ljava/lang/RuntimeException;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(npe_class);

        let aiobe_class = RuntimeClass {
            descriptor: "Ljava/lang/ArrayIndexOutOfBoundsException;".into(),
            superclass_descriptor: Some("Ljava/lang/RuntimeException;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: HashMap::new(),
            is_initialized: true,
        };
        self.register_class(aiobe_class);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_hierarchy_resolves_inheritance_and_virtual_override() {
        let mut hierarchy = ClassHierarchy::new();

        let parent = RuntimeClass {
            descriptor: "Lgame/Vehicle;".into(),
            superclass_descriptor: Some("Ljava/lang/Object;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: {
                let mut m = HashMap::new();
                m.insert(
                    ("drive".into(), "()V".into()),
                    MethodDefinition {
                        method_index: 10,
                        name: "drive".into(),
                        prototype: "()V".into(),
                        access_flags: 0x0001,
                        code_offset: 100,
                        is_direct: false,
                    },
                );
                m
            },
            is_initialized: true,
        };
        hierarchy.register_class(parent);

        let child = RuntimeClass {
            descriptor: "Lgame/Car;".into(),
            superclass_descriptor: Some("Lgame/Vehicle;".into()),
            interface_descriptors: Vec::new(),
            access_flags: 0x0001,
            fields: HashMap::new(),
            methods: {
                let mut m = HashMap::new();
                m.insert(
                    ("drive".into(), "()V".into()),
                    MethodDefinition {
                        method_index: 20,
                        name: "drive".into(),
                        prototype: "()V".into(),
                        access_flags: 0x0001,
                        code_offset: 200,
                        is_direct: false,
                    },
                );
                m
            },
            is_initialized: true,
        };
        hierarchy.register_class(child);

        assert!(hierarchy.is_assignable_from("Lgame/Vehicle;", "Lgame/Car;"));
        assert!(hierarchy.is_assignable_from("Ljava/lang/Object;", "Lgame/Car;"));
        assert!(!hierarchy.is_assignable_from("Lgame/Car;", "Lgame/Vehicle;"));

        // Virtual dispatch on child finds overridden method 20
        let (_, resolved) = hierarchy.resolve_virtual_method("Lgame/Car;", "drive", "()V").unwrap();
        assert_eq!(resolved.method_index, 20);

        // Super call from child finds parent method 10
        let (_, super_m) = hierarchy.resolve_super_method("Lgame/Car;", "drive", "()V").unwrap();
        assert_eq!(super_m.method_index, 10);
    }
    fn interface_with_method(
        descriptor: &str,
        parents: Vec<String>,
        method_index: u32,
        method_flags: u32,
    ) -> RuntimeClass {
        RuntimeClass {
            descriptor: descriptor.into(),
            superclass_descriptor: Some("Ljava/lang/Object;".into()),
            interface_descriptors: parents,
            access_flags: 0x0601,
            fields: HashMap::new(),
            methods: {
                let mut methods = HashMap::new();
                methods.insert(
                    ("render".into(), "()V".into()),
                    MethodDefinition {
                        method_index,
                        name: "render".into(),
                        prototype: "()V".into(),
                        access_flags: method_flags,
                        code_offset: 0,
                        is_direct: false,
                    },
                );
                methods
            },
            is_initialized: true,
        }
    }

    #[test]
    fn interface_dispatch_prefers_most_specific_default_method() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.register_class(interface_with_method("Lgame/BaseView;", vec![], 10, 0x0001));
        hierarchy.register_class(interface_with_method(
            "Lgame/AnimatedView;", vec!["Lgame/BaseView;".into()], 20, 0x0001
        ));
        let mut widget = RuntimeClass::new("Lgame/Widget;", Some("Ljava/lang/Object;"));
        widget.interface_descriptors = vec!["Lgame/AnimatedView;".into()];
        hierarchy.register_class(widget);

        let (owner, method) = hierarchy.resolve_virtual_method("Lgame/Widget;", "render", "()V").unwrap();
        assert_eq!(owner.descriptor, "Lgame/AnimatedView;");
        assert_eq!(method.method_index, 20);
    }

    #[test]
    fn abstract_subinterface_redeclaration_suppresses_parent_default() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.register_class(interface_with_method("Lgame/BaseView;", vec![], 10, 0x0001));
        hierarchy.register_class(interface_with_method(
            "Lgame/AbstractView;", vec!["Lgame/BaseView;".into()], 20, 0x0401
        ));
        let mut widget = RuntimeClass::new("Lgame/Widget;", Some("Ljava/lang/Object;"));
        widget.interface_descriptors = vec!["Lgame/AbstractView;".into()];
        hierarchy.register_class(widget);

        assert!(hierarchy.resolve_virtual_method("Lgame/Widget;", "render", "()V").is_none());
    }

    #[test]
    fn unrelated_interface_defaults_are_not_selected_by_traversal_order() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.register_class(interface_with_method("Lgame/Left;", vec![], 10, 0x0001));
        hierarchy.register_class(interface_with_method("Lgame/Right;", vec![], 20, 0x0001));
        let mut widget = RuntimeClass::new("Lgame/Widget;", Some("Ljava/lang/Object;"));
        widget.interface_descriptors = vec!["Lgame/Left;".into(), "Lgame/Right;".into()];
        hierarchy.register_class(widget);

        assert!(hierarchy.resolve_virtual_method("Lgame/Widget;", "render", "()V").is_none());
    }

}
