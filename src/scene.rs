use super::Object;

pub struct Scene {
    objects: Vec<Object>,
    modified: bool,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            modified: false,
        }
    }

    pub fn add(&mut self, object: Object) {
        self.objects.push(object);
        self.modified = true;
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn objects_mut(&mut self) -> &mut [Object] {
        &mut self.objects
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn clear_modified(&mut self) {
        self.modified = false
    }
}
