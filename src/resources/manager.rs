use std::{path::Path, rc::Rc};

use super::*;
use crate::{material::*, vulkan::Pipeline, Mesh};

use crate::resources;
use crate::vulkan;
use crate::Error;
use vulkan::descriptors::*;
use vulkan::Texture;
use vulkan::VulkanContext;

pub struct ResourceManager {
    context: Rc<VulkanContext>,
    descriptor_allocator: DescriptorAllocator,
    descriptor_layouts: DescriptorLayoutCache,
    textures: ResourceCache<Texture>,
    materials: ResourceCache<Material>,
    effects: ResourceCache<MaterialEffect>,
    meshes: ResourceCache<Mesh>,
}

impl ResourceManager {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        let descriptor_allocator = DescriptorAllocator::new(context.device_ref(), 1024);
        let descriptor_layouts = DescriptorLayoutCache::new(context.device_ref());

        let textures = ResourceCache::new();
        let materials = ResourceCache::new();
        let effects = ResourceCache::new();
        let meshes = ResourceCache::new();

        Self {
            context,
            descriptor_allocator,
            descriptor_layouts,
            textures,
            materials,
            effects,
            meshes,
        }
    }

    /// Get a material by name.
    pub fn material<S>(&self, name: S) -> Result<Handle<Material>, resources::Error>
    where
        S: AsRef<str> + Into<String>,
    {
        self.materials.get(name)
    }

    /// Get a material effect by name.
    pub fn effect<S>(&self, name: S) -> Result<Handle<MaterialEffect>, resources::Error>
    where
        S: AsRef<str> + Into<String>,
    {
        self.effects.get(name)
    }

    /// Get a texture by name.
    pub fn texture<S>(&self, name: S) -> Result<Handle<Texture>, resources::Error>
    where
        S: AsRef<str> + Into<String>,
    {
        self.textures.get(name)
    }

    /// Get a mesh by name.
    pub fn mesh<S>(&self, name: S) -> Result<Handle<Mesh>, resources::Error>
    where
        S: AsRef<str> + Into<String>,
    {
        self.meshes.get(name)
    }

    pub fn load_material<S>(
        &mut self,
        name: S,
        info: MaterialInfo,
    ) -> Result<Handle<Material>, Error>
    where
        S: AsRef<str> + Into<String>,
    {
        let effect = self.effect(info.effect)?;
        let albedo = self.texture(info.albedo)?;

        let context = self.context.clone();
        let descriptor_layouts = &mut self.descriptor_layouts;
        let descriptor_allocator = &mut self.descriptor_allocator;
        let textures = &self.textures;

        self.materials
            .insert(name, || {
                Material::new(
                    context,
                    descriptor_layouts,
                    descriptor_allocator,
                    textures,
                    effect,
                    albedo,
                )
            })
            .map_err(|e| e.into())
    }

    pub fn load_effect<S>(
        &mut self,
        name: S,
        passes: Vec<Pipeline>,
    ) -> Result<Handle<MaterialEffect>, Error>
    where
        S: AsRef<str> + Into<String>,
    {
        self.effects
            .insert(name, || Ok(MaterialEffect::new(passes)))
    }

    pub fn load_texture<P, S>(&mut self, name: S, path: P) -> Result<Handle<Texture>, Error>
    where
        P: AsRef<Path>,
        S: AsRef<str> + Into<String>,
    {
        let context = self.context.clone();

        self.textures
            .insert(name, || Texture::load(context, path))
            .map_err(|e| e.into())
    }

    /// TODO extract gltf model
    pub fn load_mesh<S>(
        &mut self,
        name: S,
        mesh: gltf::Mesh,
        buffers: &[gltf::buffer::Data],
    ) -> Result<Handle<Mesh>, Error>
    where
        S: AsRef<str> + Into<String>,
    {
        let context = self.context.clone();

        self.meshes
            .insert(name, || Mesh::from_gltf(context, mesh, buffers))
            .map_err(|e| e.into())
    }

    /// Get a reference to the resource manager's textures.
    pub fn textures(&self) -> &ResourceCache<Texture> {
        &self.textures
    }

    /// Get a reference to the resource manager's materials.
    pub fn materials(&self) -> &ResourceCache<Material> {
        &self.materials
    }

    /// Get a reference to the resource manager's effects.
    pub fn effects(&self) -> &ResourceCache<MaterialEffect> {
        &self.effects
    }

    /// Get a reference to the resource manager's meshes.
    pub fn meshes(&self) -> &ResourceCache<Mesh> {
        &self.meshes
    }
}
