use super::*;
use serde_json::json;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BindingKind {
    SketchPlane,
    ExtrudeUpToFace,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ContextBinding {
    pub feature: String,
    pub instance: String,
    pub face: String,
    pub kind: BindingKind,
    #[serde(default)]
    pub offset: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Context {
    pub id: String,
    pub name: String,
    pub primary_instance: String,
    pub primary_source: String,
    pub primary_transform: Transform,
    pub assembly_revision: String,
    pub sources: Vec<Source>,
    pub instances: Vec<Instance>,
    pub poses: BTreeMap<String, Transform>,
    #[serde(default)]
    pub bindings: Vec<ContextBinding>,
}
impl Context {
    pub fn validate(&self) -> Result<()> {
        identity(&self.id)?;
        self.primary_transform.validate()?;
        if self.sources.len() > 128 || self.instances.len() > 1024 || self.bindings.len() > 2000 {
            return Err("Context exceeds its snapshot limits".into());
        }
        let snapshot = AssemblyDocument {
            sources: self.sources.clone(),
            instances: self.instances.clone(),
            ..Default::default()
        };
        snapshot.validate()?;
        let primary = self
            .instances
            .iter()
            .find(|i| i.id == self.primary_instance)
            .ok_or("Context primary instance is missing")?;
        if primary.source != self.primary_source || self.poses.len() != self.instances.len() {
            return Err("Context primary source or poses are invalid".into());
        }
        if !self
            .poses
            .get(&self.primary_instance)
            .is_some_and(|pose| pose.approximately(&self.primary_transform))
        {
            return Err("Context primary frame does not match its frozen instance pose".into());
        }
        for instance in &self.instances {
            self.poses
                .get(&instance.id)
                .ok_or("Frozen instance pose is missing")?
                .validate()?;
        }
        let mut bound_features = BTreeSet::new();
        for binding in &self.bindings {
            let kind = match binding.kind {
                BindingKind::SketchPlane => 0,
                BindingKind::ExtrudeUpToFace => 1,
            };
            if !bound_features.insert((&binding.feature, kind)) {
                return Err("A context has duplicate bindings for the same feature input".into());
            }
            if !self.instances.iter().any(|i| i.id == binding.instance)
                || binding.instance == self.primary_instance
                || binding.face.is_empty()
                || binding.face.len() > 2048
                || !binding.offset.is_finite()
                || binding.offset.abs() > 1e8
            {
                return Err("Invalid context feature reference".into());
            }
        }
        Ok(())
    }
    pub fn is_stale(&self, assembly: &AssemblyDocument) -> bool {
        self.assembly_revision != state_revision(assembly).unwrap_or_default()
    }
}
fn state_revision(assembly: &AssemblyDocument) -> Result<String> {
    Ok(revision(
        &serde_json::to_vec(&(&assembly.sources, &assembly.instances, &assembly.mates))
            .map_err(|e| e.to_string())?,
    ))
}
impl AssemblyRuntime {
    pub fn capture_context(
        &mut self,
        document: &AssemblyDocument,
        primary: &str,
    ) -> Result<Context> {
        let evaluated = self.evaluate(document)?;
        let primary_instance = document
            .instances
            .iter()
            .find(|i| i.id == primary)
            .ok_or("Choose a primary assembly instance")?;
        let id = (0..4096)
            .map(|i| format!("context{i}"))
            .find(|id| {
                !document.contexts.iter().any(|c| &c.id == id)
                    && !document.instances.iter().any(|i| &i.id == id)
                    && !document.sources.iter().any(|s| &s.id == id)
                    && !document.mates.iter().any(|m| &m.id == id)
            })
            .ok_or("Context identity limit reached")?;
        Ok(Context {
            id,
            name: format!("Context for {}", primary_instance.name),
            primary_instance: primary.into(),
            primary_source: primary_instance.source.clone(),
            primary_transform: evaluated.poses[primary],
            assembly_revision: state_revision(document)?,
            sources: document.sources.clone(),
            instances: document.instances.clone(),
            poses: evaluated.poses,
            bindings: vec![],
        })
    }
    fn context_sources(
        &mut self,
        context: &Context,
    ) -> Result<BTreeMap<String, Arc<EvaluatedSource>>> {
        context.validate()?;
        self.retain_sources(&context.sources);
        let mut sources = BTreeMap::new();
        for source in &context.sources {
            sources.insert(source.id.clone(), self.source(source)?);
        }
        Ok(sources)
    }
    pub fn context_scene(&mut self, context: &Context) -> Result<Arc<Scene>> {
        let sources = self.context_sources(context)?;
        let inverse = context.primary_transform.inverse();
        let poses = context
            .poses
            .iter()
            .map(|(id, pose)| (id.clone(), inverse.compose(*pose)))
            .collect();
        Ok(Arc::new(compose_scene(
            &context.instances,
            &poses,
            &sources,
            Some(&context.primary_instance),
        )?))
    }
    pub fn reference_plane(
        &mut self,
        context: &Context,
        instance: &str,
        face: &str,
    ) -> Result<Plane> {
        if instance == context.primary_instance {
            return Err("An in-context reference must belong to another instance".into());
        }
        let sources = self.context_sources(context)?;
        let snapshot = AssemblyDocument {
            sources: context.sources.clone(),
            instances: context.instances.clone(),
            ..Default::default()
        };
        let local = connector_frame(&Connector::on_face(instance, face), &snapshot, &sources)?;
        let pose = context
            .poses
            .get(instance)
            .ok_or("Referenced context instance is missing")?;
        let plane = context
            .primary_transform
            .inverse()
            .compose(*pose)
            .compose(local)
            .plane(&Plane::named("Top", 0.));
        validate_plane(&plane)?;
        Ok(plane)
    }
    pub fn apply_context(&mut self, context: &Context, document: &mut Document) -> Result<()> {
        context.validate()?;
        let mut candidate = document.clone();
        for binding in &context.bindings {
            let plane = self
                .reference_plane(context, &binding.instance, &binding.face)?
                .shifted(binding.offset);
            let feature = candidate
                .features
                .iter_mut()
                .find(|f| f.id == binding.feature)
                .ok_or_else(|| format!("In-context feature {} is missing", binding.feature))?;
            let key = match binding.kind {
                BindingKind::SketchPlane => {
                    if feature.sketch().is_none() {
                        return Err("Sketch-plane references require a sketch feature".into());
                    }
                    feature.params["support"] = json!("");
                    "context_plane"
                }
                BindingKind::ExtrudeUpToFace => {
                    if feature.plugin != "extrude" {
                        return Err("Up-to-face references require an extrude feature".into());
                    }
                    "context_up_to"
                }
            };
            feature.params[key] = serde_json::to_value(&plane).map_err(|e| e.to_string())?;
            feature.params["context_ref"] = json!({"context":context.id,"instance":binding.instance,"face":binding.face,"kind":binding.kind,"offset":binding.offset});
        }
        let evaluation = Runtime::shared(self.registry.clone()).evaluate(&candidate)?;
        if let Some(error) = &evaluation.scene.error {
            return Err(format!("In-context rebuild failed: {error}"));
        }
        *document = candidate;
        Ok(())
    }
    pub fn bind_context(
        &mut self,
        context: &mut Context,
        document: &mut Document,
        binding: ContextBinding,
    ) -> Result<()> {
        let mut candidate = context.clone();
        candidate
            .bindings
            .retain(|b| b.feature != binding.feature || b.kind != binding.kind);
        candidate.bindings.push(binding);
        let mut edited = document.clone();
        self.apply_context(&candidate, &mut edited)?;
        *document = edited;
        *context = candidate;
        Ok(())
    }
    /// Explicit update replaces the frozen snapshot and all bound primary features together.
    pub fn update_context(&mut self, document: &mut AssemblyDocument, id: &str) -> Result<()> {
        let old = document
            .contexts
            .iter()
            .find(|c| c.id == id)
            .ok_or("Missing assembly context")?
            .clone();
        let mut context = self.capture_context(document, &old.primary_instance)?;
        context.id = old.id;
        context.name = old.name;
        context.bindings = old.bindings;
        let mut candidate = document.clone();
        let source = candidate
            .sources
            .iter_mut()
            .find(|s| s.id == context.primary_source)
            .ok_or("Missing context primary source")?;
        self.apply_context(&context, &mut source.document)?;
        source.refresh_revision()?;
        context.assembly_revision = state_revision(&candidate)?;
        *candidate.contexts.iter_mut().find(|c| c.id == id).unwrap() = context;
        self.evaluate(&candidate)?;
        *document = candidate;
        Ok(())
    }
}
