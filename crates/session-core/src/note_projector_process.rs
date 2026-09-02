//! Private, read-only `note.project` one-shot transport.
//!
//! This module is not registered with Tauri and the default library reader
//! continues to use `UnavailableProjector`.

#![allow(dead_code)]

use std::ffi::{CString, c_void};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use core_foundation::array::{CFArray, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::kCFBooleanTrue;
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::dictionary::{
    CFDictionary, CFDictionaryGetCount, CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::local_vocabulary::{
    VocabularyRangeReplacement, vocabulary_range_replacements_are_valid,
};
use crate::meeting::valid_opaque_id;
use crate::model_store::{
    DownloadableModel, ModelCatalog, NoteModel, NoteModelFile, NoteModelFileRole,
};
use crate::note_projection::{
    MAX_PROJECTION_FRAME_BYTES, NoteProjector, ProjectRequest, ProjectTransportError,
    ProjectionCancellation, StrictJson, array, exact_object, strict_json,
    string, u64_value,
};
use crate::operations::SpeakerLabelOverride;
use crate::storage::StorageRoot;

const MANIFEST_FD: RawFd = 100;
const BRIDGE_FD: RawFd = 101;
const VALIDATOR_FD: RawFd = 102;
/// Staged only for a `generate`-role launch: the manifest-pinned generator
/// bytes.  The descriptor set and the role are one decision — the bridge
/// refuses a generate manifest without this descriptor and a project manifest
/// with it.
const GENERATOR_FD: RawFd = 103;
const FIRST_STAGING_FD: RawFd = 104;
/// The registered whole-run generation budget (`candidate_first.PRODUCT_RUN`
/// `gates.maximum_elapsed_seconds`).  The bridge restates the same number as
/// `GENERATOR_DEADLINE_S` and refuses at ready if its validator bundle
/// disagrees, so a drifted copy here cannot silently widen the budget: the
/// bridge caps every requested deadline at its own registered bound.
const GENERATE_DEADLINE_SECONDS: u64 = 3600;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;

/// Opt-in diagnostic (`YAWN_NOTE_TRACE=1` in the app's environment): the
/// bridge child's failure frames are content-free by design, so when a
/// generate run ends `Unavailable` the only evidence is the child's stderr,
/// which the monitor otherwise counts and discards. With the variable set,
/// the last 4 KiB is kept and printed to the app's own stderr. Off by
/// default; nothing is written to disk.
pub fn note_trace_enabled() -> bool {
    std::env::var_os("YAWN_NOTE_TRACE").is_some()
}

static LAST_STDERR_TAIL: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

fn record_stderr_tail(chunk: &[u8]) {
    if let Ok(mut tail) = LAST_STDERR_TAIL.lock() {
        tail.extend_from_slice(chunk);
        if tail.len() > 4096 {
            let cut = tail.len() - 4096;
            tail.drain(..cut);
        }
    }
}

pub fn last_bridge_stderr_tail() -> String {
    LAST_STDERR_TAIL
        .lock()
        .map(|tail| String::from_utf8_lossy(&tail).into_owned())
        .unwrap_or_default()
}
const CLEANUP_GRACE: Duration = Duration::from_millis(750);
const PRODUCT_RUNTIME_PATH: &str = "python-runtime/bin/python3.12";
/// The read-only claim-projection role this transport speaks.  It never runs a
/// model, so its manifest carries no generator and no pinned models.
const PROJECT_ROLE: &str = "project";
/// The note-generation role.  Its manifest is the bundle's signed statement of
/// which generator and which model digests the app ships.
const GENERATE_ROLE: &str = "generate";
/// The bundled manifest file names, owned here so `worker/build_manifest.py`
/// (`NOTE_MANIFEST`, `NOTE_GENERATE_MANIFEST`) and every admitting caller
/// agree on one spelling per role.
pub const PROJECT_MANIFEST_FILE: &str = "note-runtime-project.json";
pub const GENERATE_MANIFEST_FILE: &str = "note-runtime-generate.json";
const PYTHON_SIGNING_IDENTIFIER_SUFFIX: &str = ".python-runtime";
const PYTHON_ENTITLEMENT: &str = "com.apple.security.cs.allow-unsigned-executable-memory";
const CODE_SIGNATURE_RUNTIME: u32 = 0x0001_0000;
const CODE_DIRECTORY_SHA256: i32 = 2;
const SECURITY_NO_NETWORK_ACCESS: u32 = 1 << 29;
const SECURITY_CHECK_ALL_ARCHITECTURES: u32 = 1 << 0;
const SECURITY_CHECK_NESTED_CODE: u32 = 1 << 3;
const SECURITY_STRICT_VALIDATE: u32 = 1 << 4;
const SECURITY_SIGNING_INFORMATION: u32 = 1 << 1;
const SECURITY_REQUIREMENT_INFORMATION: u32 = 1 << 2;
const SECURITY_DYNAMIC_INFORMATION: u32 = 1 << 3;

const BOOTSTRAP: &str = r#"import hashlib,json,os,sys,types
F=('schema','role','runtime','bridge','validator','generator','models')
def _identity(s): return (s.st_dev,s.st_ino,s.st_mode,s.st_uid,s.st_size,s.st_nlink)
def _read(fd):
 s=os.fstat(fd)
 if s.st_mode&0o170000!=0o100000 or s.st_uid!=os.geteuid() or s.st_mode&0o022: raise RuntimeError('unsafe inherited resource')
 os.lseek(fd,0,os.SEEK_SET); b=b''
 while len(b)<s.st_size:
  c=os.read(fd,min(1048576,s.st_size-len(b)))
  if not c: raise RuntimeError('short inherited resource')
  b+=c
 if _identity(os.fstat(fd))!=_identity(s): raise RuntimeError('changed inherited resource')
 return b
def _pairs(p):
 d={}
 for k,v in p:
  if k in d: raise RuntimeError('duplicate manifest field')
  d[k]=v
 return d
def _resource(v):
 if not isinstance(v,dict) or tuple(v)!=('relative_path','sha256'): raise RuntimeError('bad manifest resource')
 p=v['relative_path']; h=v['sha256']
 if not isinstance(p,str) or not p or any(c not in 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-/' for c in p) or any(c in ('','.', '..') for c in p.split('/')): raise RuntimeError('bad resource path')
 if not isinstance(h,str) or len(h)!=64 or any(c not in '0123456789abcdef' for c in h): raise RuntimeError('bad resource digest')
 return v
role=sys.argv[3]
if role not in ('project','generate'): raise RuntimeError('bad role')
raw=_read(100)
if b'\\' in raw: raise RuntimeError('manifest escape')
d=json.loads(raw.decode('utf-8'),object_pairs_hook=_pairs)
if not isinstance(d,dict) or tuple(d)!=F or d['schema']!='note-runtime/1' or d['role']!=role: raise RuntimeError('bad manifest')
if role=='project':
 if d['generator'] is not None or d['models']!=[]: raise RuntimeError('bad project manifest')
else:
 if not isinstance(d['models'],list) or not d['models']: raise RuntimeError('bad generate manifest')
 _resource(d['generator'])
for n in ('runtime','bridge','validator'): _resource(d[n])
if json.dumps(d,ensure_ascii=False,indent=2).encode()!=raw: raise RuntimeError('noncanonical manifest')
b=_read(101); v=_read(102)
if hashlib.sha256(b).hexdigest()!=d['bridge']['sha256'] or hashlib.sha256(v).hexdigest()!=d['validator']['sha256']: raise RuntimeError('inherited digest mismatch')
if role=='generate' and hashlib.sha256(_read(103)).hexdigest()!=d['generator']['sha256']: raise RuntimeError('inherited digest mismatch')
m=types.ModuleType('_lmn_note_bridge_fd'); m.__file__='<verified-bridge-fd>'
sys.modules[m.__name__]=m
exec(compile(b,'<verified-bridge-fd>','exec'),m.__dict__)
raise SystemExit(m.main_from_fds(100,101,102,sys.argv[2],int(sys.argv[1]),generator_fd=(103 if role=='generate' else None)))
"#;

#[derive(Clone)]
enum InterpreterAdmission {
    DevelopmentHarness,
    Product(Arc<dyn CodeAdmissionVerifier>),
}

#[derive(Clone, Copy)]
struct Deadlines {
    ready: Duration,
    project: Duration,
    /// Outer bound on the generate result: the registered whole-run budget
    /// the bridge enforces internally, plus grace for the bridge's own
    /// verification and teardown around the model run.
    generate: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(10),
            project: Duration::from_secs(30),
            generate: Duration::from_secs(GENERATE_DEADLINE_SECONDS + 60),
        }
    }
}

pub(crate) struct ProcessNoteProjector {
    storage_root: PathBuf,
    manifest_path: PathBuf,
    admission: InterpreterAdmission,
    deadlines: Deadlines,
}

impl ProcessNoteProjector {
    #[cfg(test)]
    fn development(storage_root: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            storage_root,
            manifest_path,
            admission: InterpreterAdmission::DevelopmentHarness,
            deadlines: Deadlines::default(),
        }
    }

    fn product(storage_root: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            storage_root,
            manifest_path,
            admission: InterpreterAdmission::Product(Arc::new(SecurityCodeVerifier)),
            deadlines: Deadlines::default(),
        }
    }

    #[cfg(test)]
    fn product_with_verifier(
        storage_root: PathBuf,
        manifest_path: PathBuf,
        verifier: Arc<dyn CodeAdmissionVerifier>,
    ) -> Self {
        Self {
            storage_root,
            manifest_path,
            admission: InterpreterAdmission::Product(verifier),
            deadlines: Deadlines::default(),
        }
    }

    fn project_inner(
        &self,
        request: &ProjectRequest,
        cancellation: &ProjectionCancellation,
    ) -> Result<Vec<u8>, InternalOutcome> {
        if cancellation.is_cancelled() {
            return Err(InternalOutcome::Cancelled);
        }
        validate_storage_root(&self.storage_root)?;
        validate_request(request)?;
        let mut runtime = verify_manifest(&self.manifest_path, PROJECT_ROLE)?;
        drive_bridge_child(
            &mut runtime,
            &self.storage_root,
            &self.admission,
            PROJECT_ROLE,
            project_command(request)?,
            self.deadlines.ready,
            self.deadlines.project,
            cancellation,
        )
    }
}

/// Drives one verified bridge child from spawn to result, identically for
/// both roles.  The role decides exactly one structural difference -- whether
/// the manifest-pinned generator bytes are staged as a fourth descriptor --
/// and everything else (interpreter admission, seatbelt, descriptor
/// bootstrap, ready handshake, re-verification cadence, teardown) is one
/// sequence, so a hardening fix can never land on one role and miss the
/// other.
#[allow(clippy::too_many_arguments)]
fn drive_bridge_child(
    runtime: &mut VerifiedRuntime,
    storage_root: &Path,
    admission: &InterpreterAdmission,
    role: &'static str,
    command_bytes: Vec<u8>,
    ready_deadline: Duration,
    result_deadline: Duration,
    cancellation: &ProjectionCancellation,
) -> Result<Vec<u8>, InternalOutcome> {
    let prepared_admission = prepare_interpreter_admission(runtime, admission)?;
    runtime.require_unchanged()?;

    let mut sources = vec![
        runtime.manifest.file.as_raw_fd(),
        runtime.bridge.file.as_raw_fd(),
        runtime.validator.file.as_raw_fd(),
    ];
    // The descriptor set and the role are one decision, mirrored by the
    // bridge and the bootstrap: generate stages the generator bytes,
    // project must not have any to stage.
    if role == GENERATE_ROLE {
        let generator = runtime
            .generator
            .as_ref()
            .ok_or(InternalOutcome::Unavailable)?;
        sources.push(generator.file.as_raw_fd());
    } else if runtime.generator.is_some() {
        return Err(InternalOutcome::Unavailable);
    }
    let staged = stage_descriptors(&sources)?;
    let mut inherited = [ABSENT_DESCRIPTOR_MAPPING; 4];
    for (index, target) in [MANIFEST_FD, BRIDGE_FD, VALIDATOR_FD, GENERATOR_FD]
        .into_iter()
        .take(staged.len())
        .enumerate()
    {
        inherited[index] = (staged[index].as_raw_fd(), target);
    }
    let mut command = Command::new(&runtime.executable.path);
    command
        .args(["-I", "-S", "-E", "-s", "-B", "-c", BOOTSTRAP])
        .arg(std::process::id().to_string())
        .arg(storage_root)
        .arg(role)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        command.pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            deny_network_in_child()?;
            install_descriptor_mappings(inherited)
        });
    }
    let mut child = command.spawn().map_err(|_| InternalOutcome::Unavailable)?;
    let stdin = child.stdin.take().ok_or(InternalOutcome::Unavailable)?;
    let stdout = child.stdout.take().ok_or(InternalOutcome::Unavailable)?;
    let stderr = child.stderr.take().ok_or(InternalOutcome::Unavailable)?;
    let stderr_monitor = StderrMonitor::start(stderr);
    let mut guard = ChildGuard::new(child, stderr_monitor);
    let live_code = match prepared_admission.as_deref() {
        Some(prepared) => Some(bind_live_code(
            prepared,
            guard.pid(),
            &SystemProcessStartTimeInspector,
            &runtime.executable,
        )?),
        None => None,
    };
    runtime.require_unchanged()?;

    let ready_deadline = Instant::now() + ready_deadline;
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let ready_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let frame = read_bounded_line(&mut reader);
        let _ = ready_sender.send((frame, reader));
    });
    let (ready, reader) = wait_receiver(&ready_receiver, ready_deadline, cancellation, &mut guard)?;
    let _ = ready_thread.join();
    parse_ready(
        &ready.map_err(|_| InternalOutcome::Unavailable)?,
        &runtime.manifest.digest,
        role,
    )?;
    runtime.require_unchanged()?;
    if let Some(binding) = live_code.as_ref() {
        binding.require_same_process(
            guard.pid(),
            &SystemProcessStartTimeInspector,
            &runtime.executable,
        )?;
    }
    guard.require_unexited()?;
    runtime.require_unchanged()?;
    if let Some(binding) = live_code.as_ref() {
        binding.require_same_process(
            guard.pid(),
            &SystemProcessStartTimeInspector,
            &runtime.executable,
        )?;
    }

    let mut stdin = stdin;
    stdin
        .write_all(&command_bytes)
        .map_err(|_| InternalOutcome::Unavailable)?;
    drop(stdin);

    let result_deadline = Instant::now() + result_deadline;
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let result_thread = std::thread::spawn(move || {
        let _ = result_sender.send(read_to_exact_eof(reader));
    });
    let result = wait_receiver(&result_receiver, result_deadline, cancellation, &mut guard)?;
    let _ = result_thread.join();
    let result = result.map_err(|_| InternalOutcome::Unavailable)?;
    runtime.require_unchanged()?;
    if !guard.finish_success(result_deadline, cancellation)? {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(result)
}

/// The note-runtime model identifiers a catalog entry provides, one per file.
///
/// The identifiers are note-scoped on purpose: `runtime_models` already names
/// the same files for the transcription worker, and the two runtimes must not
/// be able to satisfy each other's manifest by accident.
///
/// Weights shards derive distinct identifiers from the shard designator in
/// their catalog-validated file name (`model-00001-of-00002.safetensors` →
/// `note-generator-weights-00001-of-00002`); a single-file model keeps the
/// bare `note-generator-weights`. `worker/build_manifest.py`
/// (`note_runtime_model_id`) derives the same identifiers when it writes the
/// `generate` manifest — admission compares the two derivations digest for
/// digest, so a drift between them refuses rather than mis-admits. Any
/// duplicate identifier — expressible only through weights names outside the
/// shard shape — collapses the derivation to empty, which no valid manifest's
/// non-empty pinned set can match: an honest refusal, never an ambiguous one.
fn note_runtime_models(entry: &NoteModel) -> Vec<RuntimeManifestModel> {
    let mut models: Vec<_> = entry
        .files
        .iter()
        .map(|file| RuntimeManifestModel {
            id: note_runtime_model_id(file),
            sha256: file.sha256.clone(),
        })
        .collect();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    if models.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Vec::new();
    }
    models
}

fn note_runtime_model_id(file: &NoteModelFile) -> String {
    match file.role {
        NoteModelFileRole::Config => "note-generator-config".to_owned(),
        NoteModelFileRole::Weights => match file
            .name
            .strip_prefix("model-")
            .and_then(|rest| rest.strip_suffix(".safetensors"))
        {
            Some(shard) if !shard.is_empty() => {
                format!("note-generator-weights-{}", shard.replace('.', "-"))
            }
            _ => "note-generator-weights".to_owned(),
        },
        NoteModelFileRole::WeightsIndex => "note-generator-weights-index".to_owned(),
        NoteModelFileRole::Tokenizer => "note-generator-tokenizer".to_owned(),
        NoteModelFileRole::TokenizerConfig => "note-generator-tokenizer-config".to_owned(),
    }
}

/// Chooses the `note.project` transport for one library snapshot.
///
/// The default is `UnavailableProjector` and stays the default unless every
/// condition below holds.  A missing note model is an honest refusal -- the
/// same refusal the reader already renders -- never a hang and never a note
/// the product cannot substantiate.
///
/// 1. The `generate`-role manifest verifies and pins a generator with at least
///    one model file.  The bundle ships only a `project`-role manifest today,
///    so this condition alone keeps the shipped default unchanged.
/// 2. Exactly one signed-catalog model provides that pinned set, digest for
///    digest.
/// 3. That model is installed in the private store and re-verified now by
///    `model_store::verify_note_model_directory`, not merely recorded as
///    present.
/// 4. The `project`-role manifest, which is the one this transport actually
///    hands to the child, verifies on its own terms.
///
/// The two manifests are separate because the roles admit different shapes.
/// `worker/note_bridge.py` refuses a generator on the `project` role, and the
/// descriptor transport this projector uses is `project`-only.  Reading the
/// `generate` manifest here is how Rust learns which model the bundle pins
/// without handing that manifest to this child; the signed model catalog
/// carries no note-model role yet, so it is the only build-time pin available.
/// `None` is refusal, not an error: the caller chooses whether refusal is
/// cacheable.  Success may be held for the process lifetime (the projector
/// re-verifies its manifest on every launch); refusal must be re-derived so a
/// catalog or model that arrives mid-session admits without a restart.
pub fn admit_note_projector(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
    project_manifest_path: &Path,
    generate_manifest_path: &Path,
) -> Option<Arc<dyn NoteProjector>> {
    admitted_process_projector(
        storage,
        catalog,
        project_manifest_path,
        generate_manifest_path,
    )
    .map(|projector| Arc::new(projector) as Arc<dyn NoteProjector>)
}

fn admitted_process_projector(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
    project_manifest_path: &Path,
    generate_manifest_path: &Path,
) -> Option<ProcessNoteProjector> {
    let generate = verify_manifest(generate_manifest_path, GENERATE_ROLE).ok()?;
    generate.generator.as_ref()?;
    let mut pinned = generate.models;
    if pinned.is_empty() {
        return None;
    }
    pinned.sort_by(|left, right| left.id.cmp(&right.id));
    let mut admitted = catalog
        .note_models
        .iter()
        .filter(|entry| note_runtime_models(entry) == pinned);
    let entry = admitted.next()?;
    if admitted.next().is_some() {
        return None;
    }
    let directory = storage.resolve(&entry.relative_path()).ok()?;
    entry.verify_directory(&directory).ok()?;
    verify_manifest(project_manifest_path, PROJECT_ROLE).ok()?;
    Some(ProcessNoteProjector::product(
        storage.path().to_path_buf(),
        project_manifest_path.to_path_buf(),
    ))
}

/// One `note.generate` request for the hardened one-shot child.
///
/// It names no note -- the note is what the run is asked to propose -- and no
/// model: the admitted generator already knows which installed directory its
/// manifest pins, so a caller cannot point the run at different weights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateNoteRequest {
    pub request_id: Uuid,
    pub meeting_id: String,
    pub transcript_sha256: String,
    /// A bounded, operation-scoped presentation overlay. It is not a
    /// transcript derivative: locators and note provenance stay bound to the
    /// retained transcript digest above.
    pub speaker_label_overrides: Vec<SpeakerLabelOverride>,
    /// Exact source-span substitutions for model-facing prompt text. The
    /// worker re-derives every range and digest from the retained transcript;
    /// candidate locators and note provenance never point at replacement text.
    pub vocabulary_replacements: Vec<VocabularyRangeReplacement>,
    /// The operator's own labeled framing for this meeting. Prompt-only, like
    /// the overlays above: it may shift which retained excerpts the child
    /// emphasizes, and it can mint no locator, no claim, and no evidence of
    /// its own. See `operations::NoteCreateWorkerArgs::pre_meeting_context`.
    pub pre_meeting_context: Option<String>,
}

/// The `note.generate` transport: the same hardened one-shot child as the
/// projector, launched on the generate manifest with the manifest-pinned
/// generator bytes staged as a fourth inherited descriptor.
///
/// Admission (`admit_note_generator`) is where the model directory is chosen
/// and verified; a constructed generator re-verifies its manifest on every
/// launch but trusts the admission-time directory choice, exactly as the
/// projector trusts its admission.  The returned bytes are the child's raw
/// `note-generation-result/1` frame, parsed downstream.
pub struct ProcessNoteGenerator {
    storage_root: PathBuf,
    manifest_path: PathBuf,
    admission: InterpreterAdmission,
    deadlines: Deadlines,
    /// Storage-root-relative installed model directory, verified at
    /// admission against the signed catalog entry that provides the
    /// manifest's pinned set.
    model_directory: String,
}

impl ProcessNoteGenerator {
    #[cfg(test)]
    fn development(storage_root: PathBuf, manifest_path: PathBuf, model_directory: String) -> Self {
        Self {
            storage_root,
            manifest_path,
            admission: InterpreterAdmission::DevelopmentHarness,
            deadlines: Deadlines::default(),
            model_directory,
        }
    }

    pub fn generate(
        &self,
        request: &GenerateNoteRequest,
    ) -> Result<Vec<u8>, ProjectTransportError> {
        self.generate_with_cancellation(request, &ProjectionCancellation::default())
    }

    pub fn generate_with_cancellation(
        &self,
        request: &GenerateNoteRequest,
        cancellation: &ProjectionCancellation,
    ) -> Result<Vec<u8>, ProjectTransportError> {
        self.generate_inner(request, cancellation)
            .map_err(|outcome| match outcome {
                InternalOutcome::Unavailable => {
                    if note_trace_enabled() {
                        eprintln!(
                            "[note-trace] generate child unavailable; bridge stderr tail:\n{}",
                            last_bridge_stderr_tail()
                        );
                    }
                    ProjectTransportError::Unavailable
                }
                InternalOutcome::Cancelled => ProjectTransportError::Cancelled,
            })
    }

    fn generate_inner(
        &self,
        request: &GenerateNoteRequest,
        cancellation: &ProjectionCancellation,
    ) -> Result<Vec<u8>, InternalOutcome> {
        if cancellation.is_cancelled() {
            return Err(InternalOutcome::Cancelled);
        }
        validate_storage_root(&self.storage_root)?;
        validate_generate_request(request)?;
        if !valid_relative_path(&self.model_directory) {
            return Err(InternalOutcome::Unavailable);
        }
        let mut runtime = verify_manifest(&self.manifest_path, GENERATE_ROLE)?;
        drive_bridge_child(
            &mut runtime,
            &self.storage_root,
            &self.admission,
            GENERATE_ROLE,
            generate_command(request, &self.model_directory)?,
            self.deadlines.ready,
            self.deadlines.generate,
            cancellation,
        )
    }
}

/// One parsed `note-generation-result/1` frame.
///
/// `Generated` carries the child's `generation` object verbatim as JSON: the
/// desktop forwards it into the worker's `note.create` arguments, where the
/// frozen contract deep-validates the shape and the deterministic assembler
/// re-derives every digest from the retained transcript.  Nothing in it is
/// trusted here beyond the envelope; parsing it into typed fields would only
/// duplicate the worker's authority.
#[derive(Debug, Clone, PartialEq)]
pub enum NoteGenerationChildOutcome {
    Generated(serde_json::Value),
    TranscriptOnly { code: String, recoverable: bool },
}

/// Parses the generate child's raw result frame against the request it
/// answers.  Any envelope violation -- wrong schema, foreign request id, a
/// generation that disagrees with the pinned transcript, an outcome/payload
/// mismatch -- is `Unavailable`: the sandboxed child broke its contract, and
/// no product decision can be read from a broken frame.
pub fn parse_note_generation_result(
    frame: &[u8],
    request: &GenerateNoteRequest,
) -> Result<NoteGenerationChildOutcome, ProjectTransportError> {
    if frame.is_empty()
        || frame.len() > MAX_PROJECTION_FRAME_BYTES
        || !frame.ends_with(b"\n")
        || frame[..frame.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err(ProjectTransportError::Unavailable);
    }
    let root: serde_json::Value = serde_json::from_slice(&frame[..frame.len() - 1])
        .map_err(|_| ProjectTransportError::Unavailable)?;
    let serde_json::Value::Object(values) = &root else {
        return Err(ProjectTransportError::Unavailable);
    };
    let expected_keys = [
        "schema",
        "request_id",
        "operation",
        "outcome",
        "generation",
        "failure",
    ];
    if values.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !values.contains_key(*key))
    {
        return Err(ProjectTransportError::Unavailable);
    }
    let text = |key: &str| values.get(key).and_then(serde_json::Value::as_str);
    if text("schema") != Some("note-generation-result/1")
        || text("operation") != Some("note.generate")
        || text("request_id")
            .and_then(|value| Uuid::try_parse(value).ok())
            .filter(|value| {
                *value == request.request_id
                    && text("request_id") == Some(value.to_string().as_str())
            })
            .is_none()
    {
        return Err(ProjectTransportError::Unavailable);
    }
    let generation = values.get("generation").expect("checked key");
    let failure = values.get("failure").expect("checked key");
    match text("outcome") {
        Some("generated") => {
            if !failure.is_null() {
                return Err(ProjectTransportError::Unavailable);
            }
            let serde_json::Value::Object(fields) = generation else {
                return Err(ProjectTransportError::Unavailable);
            };
            // The envelope binds the payload to the request's pinned
            // transcript; every deeper field is the worker contract's to
            // judge.
            if fields.get("transcript_sha256").and_then(serde_json::Value::as_str)
                != Some(request.transcript_sha256.as_str())
            {
                return Err(ProjectTransportError::Unavailable);
            }
            Ok(NoteGenerationChildOutcome::Generated(generation.clone()))
        }
        Some("transcript-only") => {
            if !generation.is_null() {
                return Err(ProjectTransportError::Unavailable);
            }
            let serde_json::Value::Object(fields) = failure else {
                return Err(ProjectTransportError::Unavailable);
            };
            if fields.len() != 3 || !fields.contains_key("receipt") {
                return Err(ProjectTransportError::Unavailable);
            }
            let code = fields
                .get("code")
                .and_then(serde_json::Value::as_str)
                .filter(|code| !code.is_empty())
                .ok_or(ProjectTransportError::Unavailable)?;
            let recoverable = fields
                .get("recoverable")
                .and_then(serde_json::Value::as_bool)
                .ok_or(ProjectTransportError::Unavailable)?;
            Ok(NoteGenerationChildOutcome::TranscriptOnly {
                code: code.to_owned(),
                recoverable,
            })
        }
        _ => Err(ProjectTransportError::Unavailable),
    }
}

/// Chooses the `note.generate` transport, under exactly the admission rules
/// of `admit_note_projector` -- the same manifest verification, the same
/// one-catalog-entry match, the same re-verified install -- differing only in
/// what is constructed: the generate-role launcher bound to the verified
/// model directory, rather than a project-role projector.  `None` is refusal,
/// with the same caching contract: success may be held, refusal must be
/// re-derived.
pub fn admit_note_generator(
    storage: &StorageRoot,
    catalog: &ModelCatalog,
    generate_manifest_path: &Path,
) -> Option<ProcessNoteGenerator> {
    let generate = verify_manifest(generate_manifest_path, GENERATE_ROLE).ok()?;
    generate.generator.as_ref()?;
    let mut pinned = generate.models;
    if pinned.is_empty() {
        return None;
    }
    pinned.sort_by(|left, right| left.id.cmp(&right.id));
    let mut admitted = catalog
        .note_models
        .iter()
        .filter(|entry| note_runtime_models(entry) == pinned);
    let entry = admitted.next()?;
    if admitted.next().is_some() {
        return None;
    }
    let relative = entry.relative_path();
    let directory = storage.resolve(&relative).ok()?;
    entry.verify_directory(&directory).ok()?;
    let model_directory = relative.to_str()?.to_owned();
    if !valid_relative_path(&model_directory) {
        return None;
    }
    Some(ProcessNoteGenerator {
        storage_root: storage.path().to_path_buf(),
        manifest_path: generate_manifest_path.to_path_buf(),
        admission: InterpreterAdmission::Product(Arc::new(SecurityCodeVerifier)),
        deadlines: Deadlines::default(),
        model_directory,
    })
}

fn stage_descriptors(sources: &[RawFd]) -> Result<Vec<File>, InternalOutcome> {
    let mut staged = Vec::with_capacity(sources.len());
    let mut minimum = FIRST_STAGING_FD;
    for &source in sources {
        let descriptor = unsafe { libc::fcntl(source, libc::F_DUPFD_CLOEXEC, minimum) };
        if descriptor < 0 {
            return Err(InternalOutcome::Unavailable);
        }
        minimum = descriptor
            .checked_add(1)
            .ok_or(InternalOutcome::Unavailable)?;
        staged.push(unsafe { File::from_raw_fd(descriptor) });
    }
    Ok(staged)
}

// Seatbelt entry point.  Deprecated in the headers since 10.8 with no
// replacement for sandboxing a child a parent is about to exec, and still the
// mechanism Apple's own tooling relies on for exactly that.
unsafe extern "C" {
    fn sandbox_init(
        profile: *const libc::c_char,
        flags: u64,
        errorbuf: *mut *mut libc::c_char,
    ) -> libc::c_int;
}

const SANDBOX_NAMED: u64 = 0x0001;

/// Kernel-enforced network denial for the interpreter child, called between
/// fork and exec.  The named `no-network` profile denies every socket while
/// leaving file, pipe, and GPU access alone (mlx compute verified under it),
/// and because the sandbox survives exec the pinned interpreter path is
/// unchanged — the code-signing admission chain never sees a wrapper binary.
/// Fail-closed: if the profile cannot be applied the child must not launch.
fn deny_network_in_child() -> io::Result<()> {
    let profile = c"no-network";
    let mut error: *mut libc::c_char = std::ptr::null_mut();
    if unsafe { sandbox_init(profile.as_ptr(), SANDBOX_NAMED, &mut error) } != 0 {
        return Err(io::Error::other("sandbox_init(no-network) failed"));
    }
    Ok(())
}

/// A slot in the fixed-size mapping table that carries no descriptor.  The
/// table is sized for the largest role (generate, four descriptors) and
/// filled from the front, so the sentinel keeps the `pre_exec` closure free
/// of allocation while the descriptor count stays role-dependent.
const ABSENT_DESCRIPTOR_MAPPING: (RawFd, RawFd) = (-1, -1);

fn install_descriptor_mappings(inherited: [(RawFd, RawFd); 4]) -> io::Result<()> {
    for (source, target) in inherited {
        if (source, target) == ABSENT_DESCRIPTOR_MAPPING {
            continue;
        }
        debug_assert!(source >= FIRST_STAGING_FD);
        if unsafe { libc::dup2(source, target) } == -1 {
            return Err(io::Error::last_os_error());
        }
        let flags = unsafe { libc::fcntl(target, libc::F_GETFD) };
        if flags == -1
            || unsafe { libc::fcntl(target, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } == -1
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

impl NoteProjector for ProcessNoteProjector {
    fn project(&self, request: &ProjectRequest) -> Result<Vec<u8>, ProjectTransportError> {
        self.project_with_cancellation(request, &ProjectionCancellation::default())
    }

    fn project_with_cancellation(
        &self,
        request: &ProjectRequest,
        cancellation: &ProjectionCancellation,
    ) -> Result<Vec<u8>, ProjectTransportError> {
        self.project_inner(request, cancellation)
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InternalOutcome {
    Unavailable,
    Cancelled,
}

impl From<InternalOutcome> for ProjectTransportError {
    fn from(value: InternalOutcome) -> Self {
        match value {
            InternalOutcome::Unavailable => Self::Unavailable,
            InternalOutcome::Cancelled => Self::Cancelled,
        }
    }
}

impl From<()> for InternalOutcome {
    fn from(_: ()) -> Self {
        Self::Unavailable
    }
}

struct RuntimeManifestResource {
    relative_path: String,
    sha256: String,
}

/// One model file the manifest pins for the note runtime.
///
/// Deliberately carries no path.  Note-generation weights live in the private
/// per-user model store, not beside the bundle-owned resources, so the manifest
/// names them the way `app-runtime` names externally installed models: by
/// identifier and digest, resolved beneath the storage root the child already
/// receives.
#[derive(Clone, PartialEq, Eq)]
struct RuntimeManifestModel {
    id: String,
    sha256: String,
}

struct VerifiedRuntime {
    manifest: PinnedResource,
    executable: PinnedResource,
    bridge: PinnedResource,
    validator: PinnedResource,
    /// Present only for a generator-bearing manifest.  `None` is the shipped
    /// validator-only shape and admits no note generation at all.
    generator: Option<PinnedResource>,
    models: Vec<RuntimeManifestModel>,
}

impl VerifiedRuntime {
    fn require_unchanged(&mut self) -> Result<(), InternalOutcome> {
        self.manifest.require_unchanged()?;
        self.executable.require_unchanged()?;
        self.bridge.require_unchanged()?;
        self.validator.require_unchanged()?;
        if let Some(generator) = self.generator.as_mut() {
            generator.require_unchanged()?;
        }
        Ok(())
    }
}

struct PinnedResource {
    file: File,
    path: PathBuf,
    identity: FileIdentity,
    digest: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    owner: u32,
    size: u64,
}

impl FileIdentity {
    fn from_file(file: &File) -> Result<Self, InternalOutcome> {
        let metadata = file.metadata().map_err(|_| InternalOutcome::Unavailable)?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o022 != 0
        {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            owner: metadata.uid(),
            size: metadata.len(),
        })
    }

    fn matches(self, file: &File) -> bool {
        file.metadata().is_ok_and(|metadata| {
            metadata.is_file()
                && self.device == metadata.dev()
                && self.inode == metadata.ino()
                && self.mode == metadata.mode()
                && self.owner == metadata.uid()
                && self.size == metadata.len()
        })
    }
}

impl PinnedResource {
    fn open(
        root: &File,
        root_path: &Path,
        resource: &RuntimeManifestResource,
    ) -> Result<Self, InternalOutcome> {
        let file = open_relative(root, &resource.relative_path, false)?;
        let identity = FileIdentity::from_file(&file)?;
        let digest = digest_file(&file, identity)?;
        if digest != resource.sha256 {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(Self {
            file,
            path: root_path.join(&resource.relative_path),
            identity,
            digest,
        })
    }

    fn require_unchanged(&mut self) -> Result<(), InternalOutcome> {
        if !self.identity.matches(&self.file)
            || digest_file(&self.file, self.identity)? != self.digest
        {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(())
    }
}

/// Verifies one `note-runtime/1` manifest against the role the caller expects.
///
/// The role is an argument, not an inference, because the two roles admit
/// different shapes and `worker/note_bridge.py` refuses the wrong pairing:
/// `project` may carry no generator and no models, `generate` must carry both.
fn verify_manifest(path: &Path, expected_role: &str) -> Result<VerifiedRuntime, InternalOutcome> {
    if !path.is_absolute() {
        return Err(InternalOutcome::Unavailable);
    }
    let root_path = path.parent().ok_or(InternalOutcome::Unavailable)?;
    let root = open_absolute_directory(root_path)?;
    let manifest_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(InternalOutcome::Unavailable)?;
    let manifest_resource = RuntimeManifestResource {
        relative_path: manifest_name.to_owned(),
        sha256: String::new(),
    };
    let manifest_file = open_relative(&root, manifest_name, false)?;
    let manifest_identity = FileIdentity::from_file(&manifest_file)?;
    if manifest_identity.size > MAX_MANIFEST_BYTES {
        return Err(InternalOutcome::Unavailable);
    }
    let manifest_bytes = read_file(&manifest_file, manifest_identity)?;
    let manifest_digest = format!("{:x}", Sha256::digest(&manifest_bytes));
    let manifest = PinnedResource {
        file: manifest_file,
        path: root_path.join(&manifest_resource.relative_path),
        identity: manifest_identity,
        digest: manifest_digest,
    };
    if manifest_bytes.contains(&b'\\') {
        return Err(InternalOutcome::Unavailable);
    }
    let document = strict_json(&manifest_bytes).map_err(|_| InternalOutcome::Unavailable)?;
    let fields = exact_object(
        &document,
        [
            "schema",
            "role",
            "runtime",
            "bridge",
            "validator",
            "generator",
            "models",
        ],
    )
    .map_err(|_| InternalOutcome::Unavailable)?;
    if string(fields[0]).map_err(|_| InternalOutcome::Unavailable)? != "note-runtime/1"
        || string(fields[1]).map_err(|_| InternalOutcome::Unavailable)? != expected_role
    {
        return Err(InternalOutcome::Unavailable);
    }
    let executable = parse_resource(fields[2])?;
    let bridge = parse_resource(fields[3])?;
    let validator = parse_resource(fields[4])?;
    let generator = match fields[5] {
        StrictJson::Null => None,
        value => Some(parse_resource(value)?),
    };
    let models = parse_models(fields[6])?;
    // A generator is admitted on the role that runs one, and only complete.
    // This mirrors `_require_generator_admission` in `worker/note_bridge.py`
    // as a biconditional: a generator without pinned models could load
    // anything on disk, and pinned models without a generator name weights
    // nothing will read.
    if (expected_role == GENERATE_ROLE) != generator.is_some()
        || generator.is_some() == models.is_empty()
    {
        return Err(InternalOutcome::Unavailable);
    }
    if canonical_manifest(
        expected_role,
        &executable,
        &bridge,
        &validator,
        generator.as_ref(),
        &models,
    )
    .as_bytes()
        != manifest_bytes
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(VerifiedRuntime {
        manifest,
        executable: PinnedResource::open(&root, root_path, &executable)?,
        bridge: PinnedResource::open(&root, root_path, &bridge)?,
        validator: PinnedResource::open(&root, root_path, &validator)?,
        generator: generator
            .as_ref()
            .map(|resource| PinnedResource::open(&root, root_path, resource))
            .transpose()?,
        models,
    })
}

fn parse_models(value: &StrictJson) -> Result<Vec<RuntimeManifestModel>, InternalOutcome> {
    let entries = array(value).map_err(|_| InternalOutcome::Unavailable)?;
    let mut models: Vec<RuntimeManifestModel> = Vec::with_capacity(entries.len());
    for entry in entries {
        let fields =
            exact_object(entry, ["id", "sha256"]).map_err(|_| InternalOutcome::Unavailable)?;
        let id = string(fields[0])
            .map_err(|_| InternalOutcome::Unavailable)?
            .to_owned();
        let sha256 = string(fields[1])
            .map_err(|_| InternalOutcome::Unavailable)?
            .to_owned();
        if !valid_model_identifier(&id)
            || !valid_digest(&sha256)
            || models
                .iter()
                .any(|model| model.id == id || model.sha256 == sha256)
        {
            return Err(InternalOutcome::Unavailable);
        }
        models.push(RuntimeManifestModel { id, sha256 });
    }
    Ok(models)
}

fn parse_resource(value: &StrictJson) -> Result<RuntimeManifestResource, InternalOutcome> {
    let fields = exact_object(value, ["relative_path", "sha256"])
        .map_err(|_| InternalOutcome::Unavailable)?;
    let relative_path = string(fields[0])
        .map_err(|_| InternalOutcome::Unavailable)?
        .to_owned();
    let sha256 = string(fields[1])
        .map_err(|_| InternalOutcome::Unavailable)?
        .to_owned();
    if !valid_relative_path(&relative_path) || !valid_digest(&sha256) {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(RuntimeManifestResource {
        relative_path,
        sha256,
    })
}

/// Renders the one canonical byte sequence a `note-runtime/1` manifest may
/// have.  It must equal `json.dumps(document, ensure_ascii=False, indent=2)`
/// byte for byte: the bootstrap and `worker/note_bridge.py` both re-derive the
/// manifest that way and refuse anything else, so a single wrong space here
/// turns every generator-bearing manifest into a silent refusal.
fn canonical_manifest(
    role: &str,
    runtime: &RuntimeManifestResource,
    bridge: &RuntimeManifestResource,
    validator: &RuntimeManifestResource,
    generator: Option<&RuntimeManifestResource>,
    models: &[RuntimeManifestModel],
) -> String {
    let generator = match generator {
        None => "null".to_owned(),
        Some(resource) => format!(
            "{{\n    \"relative_path\": \"{}\",\n    \"sha256\": \"{}\"\n  }}",
            resource.relative_path, resource.sha256,
        ),
    };
    let models = if models.is_empty() {
        "[]".to_owned()
    } else {
        format!(
            "[\n{}\n  ]",
            models
                .iter()
                .map(|model| format!(
                    "    {{\n      \"id\": \"{}\",\n      \"sha256\": \"{}\"\n    }}",
                    model.id, model.sha256,
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        )
    };
    format!(
        "{{\n  \"schema\": \"note-runtime/1\",\n  \"role\": \"{}\",\n  \"runtime\": {{\n    \"relative_path\": \"{}\",\n    \"sha256\": \"{}\"\n  }},\n  \"bridge\": {{\n    \"relative_path\": \"{}\",\n    \"sha256\": \"{}\"\n  }},\n  \"validator\": {{\n    \"relative_path\": \"{}\",\n    \"sha256\": \"{}\"\n  }},\n  \"generator\": {},\n  \"models\": {}\n}}",
        role,
        runtime.relative_path,
        runtime.sha256,
        bridge.relative_path,
        bridge.sha256,
        validator.relative_path,
        validator.sha256,
        generator,
        models,
    )
}

fn open_absolute_directory(path: &Path) -> Result<File, InternalOutcome> {
    let name =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| InternalOutcome::Unavailable)?;
    let descriptor = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW_ANY,
        )
    };
    if descriptor < 0 {
        return Err(InternalOutcome::Unavailable);
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    let metadata = file.metadata().map_err(|_| InternalOutcome::Unavailable)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(file)
}

fn open_relative(root: &File, relative: &str, directory: bool) -> Result<File, InternalOutcome> {
    if !valid_relative_path(relative) {
        return Err(InternalOutcome::Unavailable);
    }
    let name = CString::new(relative).map_err(|_| InternalOutcome::Unavailable)?;
    let descriptor = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW_ANY
                | if directory { libc::O_DIRECTORY } else { 0 },
        )
    };
    if descriptor < 0 {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn read_file(file: &File, identity: FileIdentity) -> Result<Vec<u8>, InternalOutcome> {
    let mut clone = file.try_clone().map_err(|_| InternalOutcome::Unavailable)?;
    clone.rewind().map_err(|_| InternalOutcome::Unavailable)?;
    let mut bytes = Vec::with_capacity(
        identity
            .size
            .try_into()
            .map_err(|_| InternalOutcome::Unavailable)?,
    );
    clone
        .take(identity.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| InternalOutcome::Unavailable)?;
    if bytes.len() as u64 != identity.size || !identity.matches(file) {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(bytes)
}

fn digest_file(file: &File, identity: FileIdentity) -> Result<String, InternalOutcome> {
    Ok(format!("{:x}", Sha256::digest(read_file(file, identity)?)))
}

trait RetainedLiveCode {
    fn require_same_executable(&self, executable: &PinnedResource) -> Result<(), InternalOutcome>;
}

trait PreparedCodeAdmission {
    fn bind_live(
        &self,
        pid: u32,
        executable: &PinnedResource,
    ) -> Result<Box<dyn RetainedLiveCode>, InternalOutcome>;
}

trait CodeAdmissionVerifier: Send + Sync {
    fn prepare(
        &self,
        runtime: &VerifiedRuntime,
    ) -> Result<Box<dyn PreparedCodeAdmission>, InternalOutcome>;
}

fn prepare_interpreter_admission(
    runtime: &VerifiedRuntime,
    admission: &InterpreterAdmission,
) -> Result<Option<Box<dyn PreparedCodeAdmission>>, InternalOutcome> {
    match admission {
        InterpreterAdmission::DevelopmentHarness => Ok(None),
        InterpreterAdmission::Product(verifier) => verifier.prepare(runtime).map(Some),
    }
}

fn resources_are_inside<'a>(
    root: &Path,
    resources: impl IntoIterator<Item = &'a PinnedResource>,
) -> bool {
    resources
        .into_iter()
        .all(|resource| resource.path.strip_prefix(root).is_ok())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessStartTime {
    epoch_seconds: u64,
    microseconds: u64,
    absolute_time: u64,
}

impl ProcessStartTime {
    fn is_valid(self) -> bool {
        self.epoch_seconds > 0 && self.microseconds < 1_000_000 && self.absolute_time > 0
    }
}

trait ProcessStartTimeInspector {
    fn start_time(&self, pid: u32) -> io::Result<Option<ProcessStartTime>>;
}

struct SystemProcessStartTimeInspector;

impl ProcessStartTimeInspector for SystemProcessStartTimeInspector {
    fn start_time(&self, pid: u32) -> io::Result<Option<ProcessStartTime>> {
        let pid = i32::try_from(pid).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "process identifier exceeds the platform range",
            )
        })?;
        if pid <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process identifier must be positive",
            ));
        }
        let first = read_bsd_start_time(pid)?;
        let Some((epoch_seconds, microseconds)) = first else {
            return Ok(None);
        };
        let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v0>::zeroed();
        let usage_status = unsafe {
            libc::proc_pid_rusage(
                pid,
                libc::RUSAGE_INFO_V0,
                usage.as_mut_ptr().cast::<libc::rusage_info_t>(),
            )
        };
        if usage_status != 0 {
            return Ok(None);
        }
        let usage = unsafe { usage.assume_init() };
        if read_bsd_start_time(pid)? != first {
            return Ok(None);
        }
        Ok(Some(ProcessStartTime {
            epoch_seconds,
            microseconds,
            absolute_time: usage.ri_proc_start_abstime,
        }))
    }
}

fn read_bsd_start_time(pid: i32) -> io::Result<Option<(u64, u64)>> {
    let mut information = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>())
        .map_err(|_| io::Error::other("process start-time record is too large"))?;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            information.as_mut_ptr().cast(),
            size,
        )
    };
    if read == 0 {
        return Ok(None);
    }
    if read != size {
        return Err(io::Error::other("partial process start-time record"));
    }
    let information = unsafe { information.assume_init() };
    if information.pbi_pid != pid as u32 {
        return Err(io::Error::other("process start-time record changed PID"));
    }
    Ok(Some((
        information.pbi_start_tvsec,
        information.pbi_start_tvusec,
    )))
}

fn observed_start_time(
    inspector: &dyn ProcessStartTimeInspector,
    pid: u32,
) -> Result<ProcessStartTime, InternalOutcome> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match inspector
            .start_time(pid)
            .map_err(|_| InternalOutcome::Unavailable)?
        {
            Some(start_time) if start_time.is_valid() => return Ok(start_time),
            Some(_) | None => {
                if Instant::now() >= deadline {
                    return Err(InternalOutcome::Unavailable);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

struct BoundLiveCode {
    _code: Box<dyn RetainedLiveCode>,
    start_time: ProcessStartTime,
}

impl BoundLiveCode {
    fn require_same_process(
        &self,
        pid: u32,
        inspector: &dyn ProcessStartTimeInspector,
        executable: &PinnedResource,
    ) -> Result<(), InternalOutcome> {
        if observed_start_time(inspector, pid)? != self.start_time {
            return Err(InternalOutcome::Unavailable);
        }
        self._code.require_same_executable(executable)?;
        if observed_start_time(inspector, pid)? != self.start_time {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(())
    }
}

fn bind_live_code(
    admission: &dyn PreparedCodeAdmission,
    pid: u32,
    inspector: &dyn ProcessStartTimeInspector,
    executable: &PinnedResource,
) -> Result<BoundLiveCode, InternalOutcome> {
    let before = observed_start_time(inspector, pid)?;
    let code = admission.bind_live(pid, executable)?;
    let after = observed_start_time(inspector, pid)?;
    if before != after {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(BoundLiveCode {
        _code: code,
        start_time: after,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodeIdentity {
    path: PathBuf,
    main_executable: PathBuf,
    designated_requirement: Vec<u8>,
    sha256_code_directory_identity: [u8; 20],
    team_identifier: String,
    signing_identifier: String,
    signature_flags: u32,
    python_entitlements_exact: bool,
}

struct OuterCodeIdentity {
    bundle_path: PathBuf,
    main_executable: PathBuf,
    team_identifier: String,
    signing_identifier: String,
}

fn hardened_runtime(identity: &CodeIdentity) -> bool {
    identity.signature_flags & CODE_SIGNATURE_RUNTIME == CODE_SIGNATURE_RUNTIME
}

fn verify_outer_identity_pair(
    static_identity: &CodeIdentity,
    live_identity: &CodeIdentity,
) -> Result<OuterCodeIdentity, InternalOutcome> {
    if static_identity != live_identity
        || !hardened_runtime(static_identity)
        || static_identity.team_identifier.is_empty()
        || static_identity.signing_identifier.is_empty()
        || static_identity
            .path
            .extension()
            .and_then(|value| value.to_str())
            != Some("app")
        || static_identity
            .main_executable
            .strip_prefix(&static_identity.path)
            .is_err()
    {
        return Err(InternalOutcome::Unavailable);
    }
    let current_executable = std::env::current_exe().map_err(|_| InternalOutcome::Unavailable)?;
    if current_executable != static_identity.main_executable
        || !same_regular_file(&current_executable, &static_identity.main_executable)?
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(OuterCodeIdentity {
        bundle_path: static_identity.path.clone(),
        main_executable: static_identity.main_executable.clone(),
        team_identifier: static_identity.team_identifier.clone(),
        signing_identifier: static_identity.signing_identifier.clone(),
    })
}

fn verify_runtime_static_identity(
    identity: &CodeIdentity,
    expected_path: &Path,
    outer: &OuterCodeIdentity,
) -> Result<(), InternalOutcome> {
    let expected_identifier = format!(
        "{}{}",
        outer.signing_identifier, PYTHON_SIGNING_IDENTIFIER_SUFFIX
    );
    if identity.path != expected_path
        || identity.main_executable != expected_path
        || identity.team_identifier != outer.team_identifier
        || identity.signing_identifier != expected_identifier
        || !hardened_runtime(identity)
        || !identity.python_entitlements_exact
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn verify_runtime_identity_pair(
    static_identity: &CodeIdentity,
    live_identity: &CodeIdentity,
    expected_path: &Path,
    outer: &OuterCodeIdentity,
) -> Result<(), InternalOutcome> {
    verify_runtime_static_identity(static_identity, expected_path, outer)?;
    if live_identity != static_identity {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn verify_live_executable_file(
    main_executable: &Path,
    executable: &PinnedResource,
) -> Result<(), InternalOutcome> {
    let file = open_absolute_file(main_executable)?;
    let file_identity = FileIdentity::from_file(&file)?;
    let digest = digest_file(&file, file_identity)?;
    if file_identity != executable.identity || digest != executable.digest {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn same_regular_file(left: &Path, right: &Path) -> Result<bool, InternalOutcome> {
    let left = open_absolute_file(left)?;
    let right = open_absolute_file(right)?;
    Ok(FileIdentity::from_file(&left)? == FileIdentity::from_file(&right)?)
}

struct SecurityCodeVerifier;

impl CodeAdmissionVerifier for SecurityCodeVerifier {
    fn prepare(
        &self,
        runtime: &VerifiedRuntime,
    ) -> Result<Box<dyn PreparedCodeAdmission>, InternalOutcome> {
        let outer = verified_running_outer_code()?;
        let resources = outer.bundle_path.join("Contents/Resources");
        let expected_path = resources.join(PRODUCT_RUNTIME_PATH);
        if runtime.executable.path != expected_path
            || !resources_are_inside(
                &resources,
                [&runtime.manifest, &runtime.bridge, &runtime.validator],
            )
        {
            return Err(InternalOutcome::Unavailable);
        }

        let url = CFURL::from_path(&expected_path, false).ok_or(InternalOutcome::Unavailable)?;
        let static_code = security_static_code(&url)?;
        security_static_check(
            static_code.as_ref(),
            SECURITY_CHECK_ALL_ARCHITECTURES
                | SECURITY_STRICT_VALIDATE
                | SECURITY_NO_NETWORK_ACCESS,
            None,
        )?;
        let static_identity = security_code_identity(static_code.as_ref(), true, false)?;
        verify_runtime_static_identity(&static_identity, &expected_path, &outer)?;
        let requirement = security_designated_requirement(static_code.as_ref())?;
        Ok(Box::new(SecurityPreparedAdmission {
            static_code,
            requirement,
            static_identity,
            expected_path,
            outer,
        }))
    }
}

struct SecurityPreparedAdmission {
    static_code: OwnedSecurityRef,
    requirement: OwnedSecurityRef,
    static_identity: CodeIdentity,
    expected_path: PathBuf,
    outer: OuterCodeIdentity,
}

impl PreparedCodeAdmission for SecurityPreparedAdmission {
    fn bind_live(
        &self,
        pid: u32,
        executable: &PinnedResource,
    ) -> Result<Box<dyn RetainedLiveCode>, InternalOutcome> {
        let live_code = security_live_code(pid)?;
        security_dynamic_check(
            live_code.as_ref(),
            SECURITY_NO_NETWORK_ACCESS,
            Some(self.requirement.as_ref()),
        )?;
        let live_identity = security_code_identity(live_code.as_ref(), true, true)?;
        verify_runtime_identity_pair(
            &self.static_identity,
            &live_identity,
            &self.expected_path,
            &self.outer,
        )?;
        verify_live_executable_file(&live_identity.main_executable, executable)?;
        // Keep both the static object and the live object retained across the
        // command. A transient path replacement restored between the bracketed
        // checks still requires an active same-account attacker, which the
        // frozen boundary explicitly excludes.
        let _ = self.static_code.as_ref();
        Ok(Box::new(SecurityLiveCode {
            code: live_code,
            main_executable: live_identity.main_executable,
        }))
    }
}

struct SecurityLiveCode {
    code: OwnedSecurityRef,
    main_executable: PathBuf,
}

impl RetainedLiveCode for SecurityLiveCode {
    fn require_same_executable(&self, executable: &PinnedResource) -> Result<(), InternalOutcome> {
        verify_live_executable_file(&self.main_executable, executable)
    }
}

impl Drop for SecurityLiveCode {
    fn drop(&mut self) {
        let _ = self.code.as_ref();
    }
}

struct OwnedSecurityRef(CFTypeRef);

impl OwnedSecurityRef {
    fn new(value: CFTypeRef) -> Result<Self, InternalOutcome> {
        if value.is_null() {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(Self(value))
    }

    fn as_ref(&self) -> CFTypeRef {
        self.0
    }
}

impl Drop for OwnedSecurityRef {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn security_status(status: i32) -> Result<(), InternalOutcome> {
    if status == 0 {
        Ok(())
    } else {
        Err(InternalOutcome::Unavailable)
    }
}

fn verified_running_outer_code() -> Result<OuterCodeIdentity, InternalOutcome> {
    let live = security_self_code()?;
    security_dynamic_check(live.as_ref(), SECURITY_NO_NETWORK_ACCESS, None)?;
    let static_code = security_copy_static_code(live.as_ref())?;
    security_static_check(
        static_code.as_ref(),
        SECURITY_CHECK_ALL_ARCHITECTURES
            | SECURITY_CHECK_NESTED_CODE
            | SECURITY_STRICT_VALIDATE
            | SECURITY_NO_NETWORK_ACCESS,
        None,
    )?;
    let static_identity = security_code_identity(static_code.as_ref(), false, false)?;
    let live_identity = security_code_identity(live.as_ref(), false, true)?;
    verify_outer_identity_pair(&static_identity, &live_identity)
}

fn security_self_code() -> Result<OwnedSecurityRef, InternalOutcome> {
    let mut code = std::ptr::null();
    security_status(unsafe { SecCodeCopySelf(0, &mut code) })?;
    OwnedSecurityRef::new(code)
}

fn security_copy_static_code(code: CFTypeRef) -> Result<OwnedSecurityRef, InternalOutcome> {
    let mut static_code = std::ptr::null();
    security_status(unsafe { SecCodeCopyStaticCode(code, 0, &mut static_code) })?;
    OwnedSecurityRef::new(static_code)
}

fn security_static_code(url: &CFURL) -> Result<OwnedSecurityRef, InternalOutcome> {
    let mut code = std::ptr::null();
    security_status(unsafe {
        SecStaticCodeCreateWithPath(url.as_concrete_TypeRef(), 0, &mut code)
    })?;
    OwnedSecurityRef::new(code)
}

fn security_static_check(
    code: CFTypeRef,
    flags: u32,
    requirement: Option<CFTypeRef>,
) -> Result<(), InternalOutcome> {
    security_status(unsafe {
        SecStaticCodeCheckValidity(code, flags, requirement.unwrap_or(std::ptr::null()))
    })
}

fn security_dynamic_check(
    code: CFTypeRef,
    flags: u32,
    requirement: Option<CFTypeRef>,
) -> Result<(), InternalOutcome> {
    security_status(unsafe {
        SecCodeCheckValidity(code, flags, requirement.unwrap_or(std::ptr::null()))
    })
}

fn security_live_code(pid: u32) -> Result<OwnedSecurityRef, InternalOutcome> {
    let pid = i32::try_from(pid).map_err(|_| InternalOutcome::Unavailable)?;
    let pid_number = CFNumber::from(pid);
    let pid_key = unsafe { CFString::wrap_under_get_rule(kSecGuestAttributePid) };
    let attributes = CFDictionary::from_CFType_pairs(&[(pid_key, pid_number)]);
    let mut code = std::ptr::null();
    security_status(unsafe {
        SecCodeCopyGuestWithAttributes(
            std::ptr::null(),
            attributes.as_concrete_TypeRef(),
            0,
            &mut code,
        )
    })?;
    OwnedSecurityRef::new(code)
}

fn security_designated_requirement(code: CFTypeRef) -> Result<OwnedSecurityRef, InternalOutcome> {
    let mut requirement = std::ptr::null();
    security_status(unsafe { SecCodeCopyDesignatedRequirement(code, 0, &mut requirement) })?;
    OwnedSecurityRef::new(requirement)
}

fn security_code_path(code: CFTypeRef) -> Result<PathBuf, InternalOutcome> {
    let mut url = std::ptr::null();
    security_status(unsafe { SecCodeCopyPath(code, 0, &mut url) })?;
    let url = unsafe { CFURL::wrap_under_create_rule(url) };
    url.to_path().ok_or(InternalOutcome::Unavailable)
}

fn security_signing_information(
    code: CFTypeRef,
    dynamic: bool,
) -> Result<CFDictionary<*const c_void, *const c_void>, InternalOutcome> {
    let mut information = std::ptr::null();
    let flags = SECURITY_SIGNING_INFORMATION
        | SECURITY_REQUIREMENT_INFORMATION
        | if dynamic {
            SECURITY_DYNAMIC_INFORMATION
        } else {
            0
        };
    security_status(unsafe { SecCodeCopySigningInformation(code, flags, &mut information) })?;
    if information.is_null() {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(unsafe { CFDictionary::wrap_under_create_rule(information) })
}

fn dictionary_value(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<CFTypeRef> {
    let value = unsafe { CFDictionaryGetValue(dictionary, key.cast()) };
    (!value.is_null()).then_some(value.cast())
}

fn dictionary_string(
    dictionary: CFDictionaryRef,
    key: CFStringRef,
) -> Result<String, InternalOutcome> {
    let value = dictionary_value(dictionary, key).ok_or(InternalOutcome::Unavailable)?;
    if unsafe { core_foundation::base::CFGetTypeID(value) } != CFString::type_id() {
        return Err(InternalOutcome::Unavailable);
    }
    let string = unsafe { CFString::wrap_under_get_rule(value.cast()) };
    let value = string.to_string();
    if value.is_empty() {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(value)
}

fn dictionary_number(
    dictionary: CFDictionaryRef,
    key: CFStringRef,
) -> Result<i64, InternalOutcome> {
    let value = dictionary_value(dictionary, key).ok_or(InternalOutcome::Unavailable)?;
    if unsafe { core_foundation::base::CFGetTypeID(value) } != CFNumber::type_id() {
        return Err(InternalOutcome::Unavailable);
    }
    unsafe { CFNumber::wrap_under_get_rule(value.cast::<c_void>() as CFNumberRef) }
        .to_i64()
        .ok_or(InternalOutcome::Unavailable)
}

/// Select the public CDHash whose CodeDirectory algorithm is SHA-256.
/// Security.framework intentionally exposes this identity as a 20-byte CDHash;
/// the private `cdhashes-full` dictionary is not part of the product boundary.
fn sha256_code_directory_identity(
    dictionary: CFDictionaryRef,
) -> Result<[u8; 20], InternalOutcome> {
    let algorithms = dictionary_value(dictionary, unsafe { kSecCodeInfoDigestAlgorithms })
        .ok_or(InternalOutcome::Unavailable)?;
    let hashes = dictionary_value(dictionary, unsafe { kSecCodeInfoCdHashes })
        .ok_or(InternalOutcome::Unavailable)?;
    if unsafe { core_foundation::base::CFGetTypeID(algorithms) }
        != CFArray::<*const c_void>::type_id()
        || unsafe { core_foundation::base::CFGetTypeID(hashes) }
            != CFArray::<*const c_void>::type_id()
    {
        return Err(InternalOutcome::Unavailable);
    }
    let algorithms = algorithms.cast::<c_void>() as CFArrayRef;
    let hashes = hashes.cast::<c_void>() as CFArrayRef;
    let count = unsafe { CFArrayGetCount(algorithms) };
    if count <= 0 || unsafe { CFArrayGetCount(hashes) } != count {
        return Err(InternalOutcome::Unavailable);
    }
    let mut selected = None;
    for index in 0..count {
        let algorithm: CFTypeRef = unsafe { CFArrayGetValueAtIndex(algorithms, index) }.cast();
        if algorithm.is_null()
            || unsafe { core_foundation::base::CFGetTypeID(algorithm) } != CFNumber::type_id()
        {
            return Err(InternalOutcome::Unavailable);
        }
        let algorithm =
            unsafe { CFNumber::wrap_under_get_rule(algorithm.cast::<c_void>() as CFNumberRef) }
                .to_i64()
                .ok_or(InternalOutcome::Unavailable)?;
        let hash: CFTypeRef = unsafe { CFArrayGetValueAtIndex(hashes, index) }.cast();
        if hash.is_null()
            || unsafe { core_foundation::base::CFGetTypeID(hash) } != CFData::type_id()
        {
            return Err(InternalOutcome::Unavailable);
        }
        if algorithm == i64::from(CODE_DIRECTORY_SHA256) {
            let hash = unsafe { CFData::wrap_under_get_rule(hash.cast::<c_void>() as CFDataRef) };
            let hash: [u8; 20] = hash
                .bytes()
                .try_into()
                .map_err(|_| InternalOutcome::Unavailable)?;
            if selected.replace(hash).is_some() {
                return Err(InternalOutcome::Unavailable);
            }
        }
    }
    selected.ok_or(InternalOutcome::Unavailable)
}

fn dictionary_url_path(
    dictionary: CFDictionaryRef,
    key: CFStringRef,
) -> Result<PathBuf, InternalOutcome> {
    let value = dictionary_value(dictionary, key).ok_or(InternalOutcome::Unavailable)?;
    if unsafe { core_foundation::base::CFGetTypeID(value) } != CFURL::type_id() {
        return Err(InternalOutcome::Unavailable);
    }
    unsafe { CFURL::wrap_under_get_rule(value.cast::<c_void>() as CFURLRef) }
        .to_path()
        .ok_or(InternalOutcome::Unavailable)
}

fn exact_python_entitlements(dictionary: CFDictionaryRef) -> Result<bool, InternalOutcome> {
    let entitlements = dictionary_value(dictionary, unsafe { kSecCodeInfoEntitlementsDict })
        .ok_or(InternalOutcome::Unavailable)?;
    if unsafe { core_foundation::base::CFGetTypeID(entitlements) }
        != CFDictionary::<*const c_void, *const c_void>::type_id()
    {
        return Err(InternalOutcome::Unavailable);
    }
    let entitlements = entitlements.cast::<c_void>() as CFDictionaryRef;
    if unsafe { CFDictionaryGetCount(entitlements) } != 1 {
        return Ok(false);
    }
    let key = CFString::new(PYTHON_ENTITLEMENT);
    let value = dictionary_value(entitlements, key.as_concrete_TypeRef());
    Ok(value == Some(unsafe { kCFBooleanTrue }.cast()))
}

fn requirement_bytes(requirement: CFTypeRef) -> Result<Vec<u8>, InternalOutcome> {
    let mut data = std::ptr::null();
    security_status(unsafe { SecRequirementCopyData(requirement, 0, &mut data) })?;
    if data.is_null() {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(unsafe { CFData::wrap_under_create_rule(data) }
        .bytes()
        .to_vec())
}

fn security_code_identity(
    code: CFTypeRef,
    require_python_entitlements: bool,
    dynamic: bool,
) -> Result<CodeIdentity, InternalOutcome> {
    let information = security_signing_information(code, dynamic)?;
    let dictionary = information.as_concrete_TypeRef();
    let sha256_code_directory_identity = sha256_code_directory_identity(dictionary)?;
    let requirement = security_designated_requirement(code)?;
    let signature_flags =
        u32::try_from(dictionary_number(dictionary, unsafe { kSecCodeInfoFlags })?)
            .map_err(|_| InternalOutcome::Unavailable)?;
    let python_entitlements_exact = if require_python_entitlements {
        exact_python_entitlements(dictionary)?
    } else {
        false
    };
    Ok(CodeIdentity {
        path: security_code_path(code)?,
        main_executable: dictionary_url_path(dictionary, unsafe { kSecCodeInfoMainExecutable })?,
        designated_requirement: requirement_bytes(requirement.as_ref())?,
        sha256_code_directory_identity,
        team_identifier: dictionary_string(dictionary, unsafe { kSecCodeInfoTeamIdentifier })?,
        signing_identifier: dictionary_string(dictionary, unsafe { kSecCodeInfoIdentifier })?,
        signature_flags,
        python_entitlements_exact,
    })
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecGuestAttributePid: CFStringRef;
    static kSecCodeInfoCdHashes: CFStringRef;
    static kSecCodeInfoDigestAlgorithms: CFStringRef;
    static kSecCodeInfoEntitlementsDict: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoMainExecutable: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;

    fn SecCodeCopySelf(flags: u32, code: *mut CFTypeRef) -> i32;
    fn SecCodeCopyStaticCode(code: CFTypeRef, flags: u32, static_code: *mut CFTypeRef) -> i32;
    fn SecCodeCopyGuestWithAttributes(
        host: CFTypeRef,
        attributes: CFDictionaryRef,
        flags: u32,
        code: *mut CFTypeRef,
    ) -> i32;
    fn SecCodeCheckValidity(code: CFTypeRef, flags: u32, requirement: CFTypeRef) -> i32;
    fn SecCodeCopyPath(code: CFTypeRef, flags: u32, path: *mut CFURLRef) -> i32;
    fn SecCodeCopyDesignatedRequirement(
        code: CFTypeRef,
        flags: u32,
        requirement: *mut CFTypeRef,
    ) -> i32;
    fn SecCodeCopySigningInformation(
        code: CFTypeRef,
        flags: u32,
        information: *mut CFDictionaryRef,
    ) -> i32;
    fn SecStaticCodeCreateWithPath(path: CFURLRef, flags: u32, static_code: *mut CFTypeRef) -> i32;
    fn SecStaticCodeCheckValidity(code: CFTypeRef, flags: u32, requirement: CFTypeRef) -> i32;
    fn SecRequirementCopyData(requirement: CFTypeRef, flags: u32, data: *mut CFDataRef) -> i32;
}

fn open_absolute_file(path: &Path) -> Result<File, InternalOutcome> {
    let name =
        CString::new(path.as_os_str().as_bytes()).map_err(|_| InternalOutcome::Unavailable)?;
    let descriptor = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW_ANY,
        )
    };
    if descriptor < 0 {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn validate_storage_root(path: &Path) -> Result<(), InternalOutcome> {
    if !path.is_absolute() {
        return Err(InternalOutcome::Unavailable);
    }
    let directory = open_absolute_directory(path)?;
    let metadata = directory
        .metadata()
        .map_err(|_| InternalOutcome::Unavailable)?;
    if metadata.mode() & 0o777 != 0o700 {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn validate_request(request: &ProjectRequest) -> Result<(), InternalOutcome> {
    if !valid_opaque_id(&request.meeting_id)
        || !valid_digest(&request.note_json_sha256)
        || !valid_digest(&request.note_markdown_sha256)
        || !valid_digest(&request.transcript_sha256)
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn validate_generate_request(request: &GenerateNoteRequest) -> Result<(), InternalOutcome> {
    if !valid_opaque_id(&request.meeting_id)
        || !valid_digest(&request.transcript_sha256)
        || !speaker_label_overrides_are_valid(&request.speaker_label_overrides)
        || !vocabulary_range_replacements_are_valid(&request.vocabulary_replacements)
        || !pre_meeting_context_is_valid(&request.pre_meeting_context)
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

/// Mirrors `operations::validate_pre_meeting_context` -- duplicated rather
/// than shared because that function is private to its module and this one
/// runs before the command frame is built, not as part of the UI/durable
/// contract checks. Both bounds must move together if the ceiling changes.
fn pre_meeting_context_is_valid(value: &Option<String>) -> bool {
    match value {
        None => true,
        Some(text) => {
            !text.is_empty()
                && text.len() <= 16 * 1024
                && !text
                    .chars()
                    .any(|character| character.is_control() && character != '\n' && character != '\t')
        }
    }
}

fn speaker_label_overrides_are_valid(overrides: &[SpeakerLabelOverride]) -> bool {
    if overrides.len() > 3 {
        return false;
    }
    let mut previous = None;
    for override_ in overrides {
        let rank = match override_.source_speaker.as_deref() {
            None => 0,
            Some("Me") => 1,
            Some("Them") => 2,
            _ => return false,
        };
        if previous.is_some_and(|prior| prior >= rank)
            || override_.replacement.is_empty()
            || override_.replacement.len() > 80
            || override_.replacement.trim() != override_.replacement
            || override_.replacement.chars().any(char::is_control)
        {
            return false;
        }
        previous = Some(rank);
    }
    true
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_model_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[derive(Serialize)]
struct ProjectCommand<'a> {
    schema: &'static str,
    request_id: String,
    operation: &'static str,
    arguments: ProjectArguments<'a>,
}

#[derive(Serialize)]
struct ProjectArguments<'a> {
    meeting_id: &'a str,
    note_id: &'a str,
    transcript_id: &'a str,
}

fn project_command(request: &ProjectRequest) -> Result<Vec<u8>, InternalOutcome> {
    let mut bytes = serde_json::to_vec(&ProjectCommand {
        schema: "note-bridge-command/1",
        request_id: request.request_id.to_string(),
        operation: "note.project",
        arguments: ProjectArguments {
            meeting_id: &request.meeting_id,
            note_id: &request.note_json_sha256,
            transcript_id: &request.transcript_sha256,
        },
    })
    .map_err(|_| InternalOutcome::Unavailable)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_PROJECTION_FRAME_BYTES {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(bytes)
}

#[derive(Serialize)]
struct GenerateCommand<'a> {
    schema: &'static str,
    request_id: String,
    operation: &'static str,
    arguments: GenerateArguments<'a>,
}

/// Field order is the wire contract: `worker/note_bridge.py::_parse_command`
/// compares the argument key list exactly.
#[derive(Serialize)]
struct GenerateArguments<'a> {
    meeting_id: &'a str,
    transcript_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_label_overrides: Option<&'a [SpeakerLabelOverride]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vocabulary_replacements: Option<&'a [VocabularyRangeReplacement]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pre_meeting_context: Option<&'a str>,
    model_directory: &'a str,
    deadline_s: u64,
}

fn generate_command(
    request: &GenerateNoteRequest,
    model_directory: &str,
) -> Result<Vec<u8>, InternalOutcome> {
    let mut bytes = serde_json::to_vec(&GenerateCommand {
        schema: "note-bridge-command/1",
        request_id: request.request_id.to_string(),
        operation: "note.generate",
        arguments: GenerateArguments {
            meeting_id: &request.meeting_id,
            transcript_id: &request.transcript_sha256,
            speaker_label_overrides: (!request.speaker_label_overrides.is_empty())
                .then_some(request.speaker_label_overrides.as_slice()),
            vocabulary_replacements: (!request.vocabulary_replacements.is_empty())
                .then_some(request.vocabulary_replacements.as_slice()),
            pre_meeting_context: request.pre_meeting_context.as_deref(),
            model_directory,
            deadline_s: GENERATE_DEADLINE_SECONDS,
        },
    })
    .map_err(|_| InternalOutcome::Unavailable)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_PROJECTION_FRAME_BYTES {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(bytes)
}

fn parse_ready(frame: &[u8], manifest_sha256: &str, role: &str) -> Result<(), InternalOutcome> {
    if frame.len() > MAX_PROJECTION_FRAME_BYTES
        || !frame.ends_with(b"\n")
        || frame[..frame.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err(InternalOutcome::Unavailable);
    }
    let root = strict_json(&frame[..frame.len() - 1]).map_err(|_| InternalOutcome::Unavailable)?;
    let fields = exact_object(
        &root,
        [
            "schema",
            "event",
            "protocol",
            "role",
            "manifest_sha256",
            "operations",
        ],
    )
    .map_err(|_| InternalOutcome::Unavailable)?;
    let operations = array(fields[5]).map_err(|_| InternalOutcome::Unavailable)?;
    let operation = format!("note.{role}");
    if string(fields[0]).map_err(|_| InternalOutcome::Unavailable)? != "note-bridge-event/1"
        || string(fields[1]).map_err(|_| InternalOutcome::Unavailable)? != "ready"
        || u64_value(fields[2]).map_err(|_| InternalOutcome::Unavailable)? != 1
        || string(fields[3]).map_err(|_| InternalOutcome::Unavailable)? != role
        || string(fields[4]).map_err(|_| InternalOutcome::Unavailable)? != manifest_sha256
        || operations.len() != 1
        || string(&operations[0]).map_err(|_| InternalOutcome::Unavailable)? != operation
    {
        return Err(InternalOutcome::Unavailable);
    }
    Ok(())
}

fn read_bounded_line(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed frame"));
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if frame.len() + consumed > MAX_PROJECTION_FRAME_BYTES {
            return Err(io::Error::other("oversized frame"));
        }
        frame.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if frame.ends_with(b"\n") {
            return Ok(frame);
        }
    }
}

fn read_to_exact_eof(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(MAX_PROJECTION_FRAME_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PROJECTION_FRAME_BYTES
        || !bytes.ends_with(b"\n")
        || bytes[..bytes.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err(io::Error::other("invalid terminal frame"));
    }
    Ok(bytes)
}

fn wait_receiver<T>(
    receiver: &Receiver<T>,
    deadline: Instant,
    cancellation: &ProjectionCancellation,
    guard: &mut ChildGuard,
) -> Result<T, InternalOutcome> {
    loop {
        if cancellation.is_cancelled() {
            guard.abort();
            return Err(InternalOutcome::Cancelled);
        }
        if guard.stderr_overflowed() {
            guard.abort();
            return Err(InternalOutcome::Unavailable);
        }
        match receiver.try_recv() {
            Ok(value) => return Ok(value),
            Err(TryRecvError::Disconnected) => return Err(InternalOutcome::Unavailable),
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryRecvError::Empty) => {
                guard.abort();
                return Err(InternalOutcome::Unavailable);
            }
        }
    }
}

struct StderrMonitor {
    overflow: Receiver<()>,
    thread: Option<JoinHandle<bool>>,
}

impl StderrMonitor {
    fn start(mut stderr: impl Read + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let trace = note_trace_enabled();
        if trace {
            if let Ok(mut tail) = LAST_STDERR_TAIL.lock() {
                tail.clear();
            }
        }
        let thread = std::thread::spawn(move || {
            let mut total = 0_usize;
            let mut sent = false;
            let mut buffer = [0_u8; 4096];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) => return total <= MAX_STDERR_BYTES,
                    Ok(read) => {
                        if trace {
                            record_stderr_tail(&buffer[..read]);
                        }
                        total = total.saturating_add(read);
                        if total > MAX_STDERR_BYTES && !sent {
                            let _ = sender.try_send(());
                            sent = true;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => return false,
                }
            }
        });
        Self {
            overflow: receiver,
            thread: Some(thread),
        }
    }
}

struct ChildGuard {
    child: Child,
    process_group_id: i32,
    stderr: StderrMonitor,
    finished: bool,
}

impl ChildGuard {
    fn new(child: Child, stderr: StderrMonitor) -> Self {
        Self {
            process_group_id: child.id() as i32,
            child,
            stderr,
            finished: false,
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn stderr_overflowed(&self) -> bool {
        self.stderr.overflow.try_recv().is_ok()
    }

    fn require_unexited(&mut self) -> Result<(), InternalOutcome> {
        if self
            .child
            .try_wait()
            .map_err(|_| InternalOutcome::Unavailable)?
            .is_some()
        {
            return Err(InternalOutcome::Unavailable);
        }
        Ok(())
    }

    fn finish_success(
        &mut self,
        deadline: Instant,
        cancellation: &ProjectionCancellation,
    ) -> Result<bool, InternalOutcome> {
        let status = loop {
            if cancellation.is_cancelled() {
                self.abort();
                return Err(InternalOutcome::Cancelled);
            }
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|_| InternalOutcome::Unavailable)?
            {
                if cancellation.is_cancelled() {
                    self.abort();
                    return Err(InternalOutcome::Cancelled);
                }
                break status;
            }
            if Instant::now() >= deadline {
                self.abort();
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if group_exists(self.process_group_id) {
            self.abort();
            return Ok(false);
        }
        let stderr_ok = self
            .stderr
            .thread
            .take()
            .ok_or(InternalOutcome::Unavailable)?
            .join()
            .map_err(|_| InternalOutcome::Unavailable)?;
        self.finished = true;
        Ok(success(status) && stderr_ok)
    }

    fn abort(&mut self) {
        if self.finished {
            return;
        }
        let _ = signal_group(self.process_group_id, libc::SIGTERM);
        if !wait_for_group_exit(&mut self.child, self.process_group_id, CLEANUP_GRACE) {
            let _ = signal_group(self.process_group_id, libc::SIGKILL);
            let _ = self.child.wait();
        }
        if let Some(thread) = self.stderr.thread.take() {
            let _ = thread.join();
        }
        self.finished = true;
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.abort();
    }
}

fn success(status: ExitStatus) -> bool {
    status.success()
}

fn wait_for_group_exit(child: &mut Child, group: i32, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        let _ = child.try_wait();
        if !group_exists(group) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn group_exists(group: i32) -> bool {
    let result = unsafe { libc::kill(-group, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn signal_group(group: i32, signal: i32) -> io::Result<()> {
    if unsafe { libc::kill(-group, signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Output;
    use std::sync::{Mutex, mpsc};

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::model_store::{
        INSTALL_RECEIPT_NAME, ModelCatalogSchema, TranscriptModel, TranscriptModelFile,
        TranscriptModelFileRole, note_install_receipt_bytes,
    };
    use crate::note_projection::UnavailableProjector;
    use crate::storage::{create_private_dir, durable_create_new};

    const GENERATOR_FIXTURE: &[u8] = b"generator-fixture";
    const NOTE_CONFIG_FIXTURE: &[u8] = b"note-generator-config-fixture";
    const NOTE_WEIGHTS_FIXTURE: &[u8] = b"note-generator-weights-fixture";

    fn request() -> ProjectRequest {
        ProjectRequest {
            request_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            meeting_id: "meeting-a".into(),
            note_json_sha256: "a".repeat(64),
            note_markdown_sha256: "b".repeat(64),
            transcript_sha256: "c".repeat(64),
        }
    }

    const LAUNCH_FIXTURE_BRIDGE: &str = r#"import hashlib,json,os,sys,time
def _read(fd):
 os.lseek(fd,0,os.SEEK_SET); return os.read(fd,1048576)
def _ready(manifest,role):
 print(json.dumps({'schema':'note-bridge-event/1','event':'ready','protocol':1,'role':role,'manifest_sha256':hashlib.sha256(manifest).hexdigest(),'operations':['note.'+role]},separators=(',',':')),flush=True)
def main_from_fds(manifest_fd,bridge_fd,validator_fd,storage_root,expected_parent_pid,generator_fd=None):
 mode=open(storage_root+'/fixture-mode').read()
 manifest=_read(manifest_fd); bridge=_read(bridge_fd); validator=_read(validator_fd)
 document=json.loads(manifest)
 role='generate' if generator_fd is not None else 'project'
 if document['role']!=role: return 9
 if hashlib.sha256(bridge).hexdigest()!=document['bridge']['sha256'] or hashlib.sha256(validator).hexdigest()!=document['validator']['sha256']: return 7
 if generator_fd is not None and hashlib.sha256(_read(generator_fd)).hexdigest()!=document['generator']['sha256']: return 7
 watched=(manifest_fd,bridge_fd,validator_fd)+(() if generator_fd is None else (generator_fd,))
 if not all(os.get_inheritable(fd) for fd in watched): return 8
 if mode=='ready-timeout': time.sleep(5)
 _ready(manifest,role)
 if mode=='ready-then-exit': os._exit(0)
 command=sys.stdin.buffer.readline()
 if mode=='descriptor-binding':
  open(storage_root+'/bound-command','wb').write(command)
 if mode=='result-timeout': time.sleep(5)
 if mode=='cancel-after-result':
  print('{}',flush=True); sys.stdout.close()
  open(storage_root+'/result-eof','w').write('closed')
  time.sleep(5); return 0
 print('{}',flush=True)
 return 0
"#;

    struct LaunchFixture {
        _temporary: TempDir,
        storage_root: PathBuf,
        manifest_path: PathBuf,
        generate_manifest_path: PathBuf,
        resources: PathBuf,
        storage: StorageRoot,
        catalog: ModelCatalog,
    }

    impl LaunchFixture {
        fn admit(&self) -> Arc<dyn NoteProjector> {
            admit_note_projector(
                &self.storage,
                &self.catalog,
                &self.manifest_path,
                &self.generate_manifest_path,
            )
            .unwrap_or_else(|| Arc::new(UnavailableProjector))
        }

        fn is_admitted(&self) -> bool {
            admitted_process_projector(
                &self.storage,
                &self.catalog,
                &self.manifest_path,
                &self.generate_manifest_path,
            )
            .is_some()
        }

        fn model_directory(&self) -> PathBuf {
            let entry = &self.catalog.note_models[0];
            self.storage.resolve(&entry.relative_path()).unwrap()
        }

        fn install_note_model(&self) {
            let entry = &self.catalog.note_models[0];
            let directory = self.model_directory();
            for ancestor in directory.ancestors().skip(1).collect::<Vec<_>>().iter().rev() {
                if ancestor.starts_with(self.storage.path()) && !ancestor.exists() {
                    create_private_dir(ancestor).unwrap();
                }
            }
            create_private_dir(&directory).unwrap();
            durable_create_new(&directory.join("config.json"), NOTE_CONFIG_FIXTURE).unwrap();
            durable_create_new(&directory.join("model.safetensors"), NOTE_WEIGHTS_FIXTURE)
                .unwrap();
            durable_create_new(
                &directory.join(INSTALL_RECEIPT_NAME),
                &note_install_receipt_bytes(entry),
            )
            .unwrap();
        }

        fn resource(&self, relative: &str, bytes: &[u8]) -> RuntimeManifestResource {
            RuntimeManifestResource {
                relative_path: relative.into(),
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }
        }

        fn manifest_resources(&self) -> [RuntimeManifestResource; 3] {
            [
                RuntimeManifestResource {
                    relative_path: PRODUCT_RUNTIME_PATH.into(),
                    sha256: format!(
                        "{:x}",
                        Sha256::digest(
                            fs::read(self.resources.join(PRODUCT_RUNTIME_PATH)).unwrap()
                        )
                    ),
                },
                self.resource("note-bridge.py", LAUNCH_FIXTURE_BRIDGE.as_bytes()),
                self.resource("note-validator.zip", b"validator-fixture"),
            ]
        }

        /// Writes the `project`-role manifest: the validator-only shape the
        /// packaged bundle carries and the only shape this transport speaks.
        fn write_project_manifest(&self) {
            self.write_manifest(&self.manifest_path, PROJECT_ROLE, None, &[]);
        }

        /// Writes the `generate`-role manifest Rust reads to learn which
        /// generator and model digests the bundle pins.
        fn write_generate_manifest(
            &self,
            generator: Option<&RuntimeManifestResource>,
            models: &[RuntimeManifestModel],
        ) {
            self.write_manifest(
                &self.generate_manifest_path,
                GENERATE_ROLE,
                generator,
                models,
            );
        }

        fn write_manifest(
            &self,
            path: &Path,
            role: &str,
            generator: Option<&RuntimeManifestResource>,
            models: &[RuntimeManifestModel],
        ) {
            let [runtime, bridge, validator] = self.manifest_resources();
            private_file(
                path,
                canonical_manifest(role, &runtime, &bridge, &validator, generator, models)
                    .as_bytes(),
                false,
            );
        }

        fn pinned_models(&self) -> Vec<RuntimeManifestModel> {
            note_runtime_models(&self.catalog.note_models[0])
        }

        fn generator(&self) -> RuntimeManifestResource {
            self.resource("note-generator.py", GENERATOR_FIXTURE)
        }
    }

    /// The smallest transcript entry `ModelCatalog::validate` accepts —
    /// present because a catalog without one is invalid, unused by admission.
    fn transcript_fixture_model() -> TranscriptModel {
        let revision = "a".repeat(40);
        TranscriptModel {
            id: "whisper-test".into(),
            revision: revision.clone(),
            title: "Speech model".into(),
            detail: "A local transcription model fixture.".into(),
            download_bytes: 2,
            installed_bytes: 2,
            files: vec![
                TranscriptModelFile {
                    role: TranscriptModelFileRole::Config,
                    name: "config.json".into(),
                    url: format!("https://example.test/{revision}/config.json"),
                    bytes: 1,
                    sha256: "1".repeat(64),
                },
                TranscriptModelFile {
                    role: TranscriptModelFileRole::Weights,
                    name: "weights.safetensors".into(),
                    url: format!("https://example.test/{revision}/weights.safetensors"),
                    bytes: 1,
                    sha256: "2".repeat(64),
                },
            ],
        }
    }

    fn note_catalog() -> ModelCatalog {
        let revision = "b".repeat(40);
        let catalog = ModelCatalog {
            schema: ModelCatalogSchema::V1,
            models: vec![transcript_fixture_model()],
            note_models: vec![NoteModel {
                id: "note-generator-test".into(),
                revision: revision.clone(),
                title: "Note generator".into(),
                detail: "A local note-generation model.".into(),
                download_bytes: (NOTE_CONFIG_FIXTURE.len() + NOTE_WEIGHTS_FIXTURE.len()) as u64,
                installed_bytes: (NOTE_CONFIG_FIXTURE.len() + NOTE_WEIGHTS_FIXTURE.len()) as u64,
                files: vec![
                    NoteModelFile {
                        role: NoteModelFileRole::Config,
                        name: "config.json".into(),
                        url: format!("https://example.test/{revision}/config.json"),
                        bytes: NOTE_CONFIG_FIXTURE.len() as u64,
                        sha256: format!("{:x}", Sha256::digest(NOTE_CONFIG_FIXTURE)),
                    },
                    NoteModelFile {
                        role: NoteModelFileRole::Weights,
                        name: "model.safetensors".into(),
                        url: format!("https://example.test/{revision}/model.safetensors"),
                        bytes: NOTE_WEIGHTS_FIXTURE.len() as u64,
                        sha256: format!("{:x}", Sha256::digest(NOTE_WEIGHTS_FIXTURE)),
                    },
                ],
            }],
        };
        catalog.validate().unwrap();
        catalog
    }

    /// Re-derives canonical manifest bytes the way every consumer does:
    /// `json.dumps(document, ensure_ascii=False, indent=2)`.  This is the one
    /// contract the Rust renderer, the bootstrap, and `worker/note_bridge.py`
    /// must agree on byte for byte.
    fn python_canonical(bytes: &[u8]) -> Vec<u8> {
        let mut child = Command::new("python3")
            .args([
                "-c",
                "import json,sys\nraw=sys.stdin.buffer.read()\nsys.stdout.buffer.write(json.dumps(json.loads(raw),ensure_ascii=False,indent=2).encode())",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        output.stdout
    }

    /// The launch path's network denial, characterized against a live socket:
    /// the same connect that succeeds unsandboxed must fail once
    /// `deny_network_in_child` has run in the child.  The listener is local so
    /// the test proves kernel enforcement, not the absence of a route.
    #[test]
    fn sandboxed_child_is_denied_the_socket_an_unsandboxed_child_reaches() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let probe = format!(
            "import socket,sys\ns=socket.socket()\ntry:\n    s.connect(('127.0.0.1', {port}))\nexcept OSError:\n    sys.exit(7)\nsys.exit(0)",
        );
        let control = Command::new("python3")
            .args(["-c", &probe])
            .output()
            .unwrap();
        assert!(control.status.success(), "control connect must succeed");
        let mut sandboxed = Command::new("python3");
        sandboxed.args(["-c", &probe]);
        unsafe {
            sandboxed.pre_exec(deny_network_in_child);
        }
        let output = sandboxed.output().unwrap();
        assert_eq!(output.status.code(), Some(7), "sandboxed connect must be denied");
    }

    fn current_python() -> PathBuf {
        let output = Command::new("python3")
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        assert!(output.status.success());
        PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
    }

    fn private_file(path: &Path, bytes: &[u8], executable: bool) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
        )
        .unwrap();
    }

    /// A validator-only fixture: the shape `worker/build_manifest.py` packages
    /// today, which admits no generator and therefore no note model.
    fn launch_fixture(mode: &str) -> LaunchFixture {
        let temporary = TempDir::new().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let resources = base.join("resources");
        fs::create_dir(&resources).unwrap();
        fs::set_permissions(&resources, fs::Permissions::from_mode(0o700)).unwrap();
        let repository = base.join("repository");
        create_private_dir(&repository).unwrap();
        let storage = StorageRoot::create(&base.join("storage"), &repository).unwrap();
        let storage_root = storage.path().to_path_buf();
        private_file(&storage_root.join("fixture-mode"), mode.as_bytes(), false);

        let runtime_path = resources.join(PRODUCT_RUNTIME_PATH);
        fs::create_dir_all(runtime_path.parent().unwrap()).unwrap();
        fs::copy(current_python(), &runtime_path).unwrap();
        fs::set_permissions(&runtime_path, fs::Permissions::from_mode(0o700)).unwrap();
        private_file(
            &resources.join("note-bridge.py"),
            LAUNCH_FIXTURE_BRIDGE.as_bytes(),
            false,
        );
        private_file(
            &resources.join("note-validator.zip"),
            b"validator-fixture",
            false,
        );
        private_file(
            &resources.join("note-generator.py"),
            GENERATOR_FIXTURE,
            false,
        );

        let fixture = LaunchFixture {
            _temporary: temporary,
            storage_root,
            manifest_path: resources.join("note-runtime-project.json"),
            generate_manifest_path: resources.join("note-runtime-generate.json"),
            resources,
            storage,
            catalog: note_catalog(),
        };
        fixture.write_project_manifest();
        fixture
    }

    /// The same fixture plus a `generate`-role manifest and its pinned note
    /// model, installed and intact.  This is the shape a later bundle ships;
    /// the packaged bundle today writes only the `project` manifest.
    fn generative_fixture(mode: &str) -> LaunchFixture {
        let fixture = launch_fixture(mode);
        fixture.write_generate_manifest(Some(&fixture.generator()), &fixture.pinned_models());
        fixture.install_note_model();
        fixture
    }

    fn note_generator(
        fixture: &LaunchFixture,
        ready_ms: u64,
        generate_ms: u64,
    ) -> ProcessNoteGenerator {
        let entry = &fixture.catalog.note_models[0];
        let mut generator = ProcessNoteGenerator::development(
            fixture.storage_root.clone(),
            fixture.generate_manifest_path.clone(),
            entry.relative_path().to_str().unwrap().to_owned(),
        );
        generator.deadlines = Deadlines {
            ready: Duration::from_millis(ready_ms),
            generate: Duration::from_millis(generate_ms),
            ..Deadlines::default()
        };
        generator
    }

    fn generate_request() -> GenerateNoteRequest {
        GenerateNoteRequest {
            request_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            meeting_id: "meeting-a".into(),
            transcript_sha256: "c".repeat(64),
            speaker_label_overrides: Vec::new(),
            vocabulary_replacements: Vec::new(),
            pre_meeting_context: None,
        }
    }

    fn projector(fixture: &LaunchFixture, ready_ms: u64, project_ms: u64) -> ProcessNoteProjector {
        let mut projector = ProcessNoteProjector::development(
            fixture.storage_root.clone(),
            fixture.manifest_path.clone(),
        );
        projector.deadlines = Deadlines {
            ready: Duration::from_millis(ready_ms),
            project: Duration::from_millis(project_ms),
            ..Deadlines::default()
        };
        projector
    }

    fn launch_bootstrap(
        fixture: &LaunchFixture,
        claimed_bridge_digest: Option<&str>,
        occupy_targets: bool,
    ) -> Output {
        let mut runtime = verify_manifest(&fixture.manifest_path, PROJECT_ROLE).unwrap();
        if let Some(digest) = claimed_bridge_digest {
            let mut document: serde_json::Value =
                serde_json::from_slice(&fs::read(&fixture.manifest_path).unwrap()).unwrap();
            document["bridge"]["sha256"] = serde_json::Value::String(digest.into());
            let bytes = serde_json::to_vec_pretty(&document).unwrap();
            private_file(&fixture.manifest_path, &bytes, false);
            runtime.manifest.file = File::open(&fixture.manifest_path).unwrap();
            runtime.manifest.identity = FileIdentity::from_file(&runtime.manifest.file).unwrap();
            runtime.manifest.digest = format!("{:x}", Sha256::digest(&bytes));
        }
        let staged = stage_descriptors(&[
            runtime.manifest.file.as_raw_fd(),
            runtime.bridge.file.as_raw_fd(),
            runtime.validator.file.as_raw_fd(),
        ])
        .unwrap();
        assert!(
            staged
                .iter()
                .all(|file| file.as_raw_fd() >= FIRST_STAGING_FD)
        );
        let inherited = [
            (staged[0].as_raw_fd(), MANIFEST_FD),
            (staged[1].as_raw_fd(), BRIDGE_FD),
            (staged[2].as_raw_fd(), VALIDATOR_FD),
            ABSENT_DESCRIPTOR_MAPPING,
        ];
        let sentinel = File::open("/dev/null").unwrap();
        let sentinel_fd = sentinel.as_raw_fd();
        let mut command = Command::new(&runtime.executable.path);
        command
            .args(["-I", "-S", "-E", "-s", "-B", "-c", BOOTSTRAP])
            .arg(std::process::id().to_string())
            .arg(&fixture.storage_root)
            .arg(PROJECT_ROLE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(move || {
                if occupy_targets {
                    for target in [MANIFEST_FD, BRIDGE_FD, VALIDATOR_FD] {
                        if libc::dup2(sentinel_fd, target) == -1 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                }
                install_descriptor_mappings(inherited)
            });
        }
        command.output().unwrap()
    }

    #[test]
    fn bootstrap_refuses_a_descriptor_digest_mismatch_before_bridge_execution() {
        let fixture = launch_fixture("descriptor-binding");
        let marker = fixture.storage_root.join("bound-command");
        let output = launch_bootstrap(&fixture, Some(&"0".repeat(64)), false);
        assert!(!output.status.success());
        assert!(!marker.exists());
    }

    /// The descriptor set and the role are one decision on both sides.  A
    /// generator on the `project` role is refused by
    /// `verify_descriptor_runtime` in `worker/note_bridge.py`, so the
    /// bootstrap must refuse it too rather than admit a shape the real
    /// bridge rejects one layer down.
    #[test]
    fn bootstrap_refuses_a_generator_on_the_project_role() {
        let fixture = launch_fixture("descriptor-binding");
        let marker = fixture.storage_root.join("bound-command");
        let [runtime, bridge, validator] = fixture.manifest_resources();
        let generator = fixture.generator();
        let bytes = canonical_manifest(
            PROJECT_ROLE,
            &runtime,
            &bridge,
            &validator,
            Some(&generator),
            &fixture.pinned_models(),
        )
        .into_bytes();
        let mut verified = verify_manifest(&fixture.manifest_path, PROJECT_ROLE).unwrap();
        private_file(&fixture.manifest_path, &bytes, false);
        verified.manifest.file = File::open(&fixture.manifest_path).unwrap();
        verified.manifest.identity = FileIdentity::from_file(&verified.manifest.file).unwrap();
        let staged = stage_descriptors(&[
            verified.manifest.file.as_raw_fd(),
            verified.bridge.file.as_raw_fd(),
            verified.validator.file.as_raw_fd(),
        ])
        .unwrap();
        let inherited = [
            (staged[0].as_raw_fd(), MANIFEST_FD),
            (staged[1].as_raw_fd(), BRIDGE_FD),
            (staged[2].as_raw_fd(), VALIDATOR_FD),
            ABSENT_DESCRIPTOR_MAPPING,
        ];
        let mut command = Command::new(&verified.executable.path);
        command
            .args(["-I", "-S", "-E", "-s", "-B", "-c", BOOTSTRAP])
            .arg(std::process::id().to_string())
            .arg(&fixture.storage_root)
            .arg(PROJECT_ROLE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(move || install_descriptor_mappings(inherited));
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(!marker.exists());
    }

    /// The generate lane end-to-end on the development harness: the fourth
    /// descriptor stages the manifest-pinned generator bytes, the child
    /// answers ready on the generate role, and the command it receives names
    /// the admitted model directory and the registered deadline.
    #[test]
    fn an_admitted_generator_drives_the_child_on_the_generate_manifest() {
        let fixture = generative_fixture("descriptor-binding");
        let transport = note_generator(&fixture, 2000, 2000);
        let result = transport.generate(&generate_request()).unwrap();
        assert_eq!(result, b"{}\n");
        let bound = fs::read(fixture.storage_root.join("bound-command")).unwrap();
        let entry = &fixture.catalog.note_models[0];
        let expected = generate_command(
            &generate_request(),
            entry.relative_path().to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(bound, expected);
    }

    /// Role and manifest must agree before any child exists: pointing the
    /// generate transport at the bundled project manifest refuses at
    /// `verify_manifest`, never launching an interpreter.
    #[test]
    fn a_generate_launch_refuses_a_project_manifest() {
        let fixture = generative_fixture("descriptor-binding");
        let mut transport = note_generator(&fixture, 2000, 2000);
        transport.manifest_path = fixture.manifest_path.clone();
        assert!(matches!(
            transport.generate(&generate_request()),
            Err(ProjectTransportError::Unavailable)
        ));
        assert!(!fixture.storage_root.join("bound-command").exists());
    }

    #[test]
    fn generate_command_is_closed_and_bounded() {
        assert_eq!(
            generate_command(&generate_request(), "models/note.d/m/r").unwrap(),
            format!(
                "{{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.generate\",\"arguments\":{{\"meeting_id\":\"meeting-a\",\"transcript_id\":\"{}\",\"model_directory\":\"models/note.d/m/r\",\"deadline_s\":3600}}}}\n",
                "c".repeat(64)
            )
            .into_bytes()
        );
    }

    #[test]
    fn generate_command_carries_only_a_bounded_speaker_label_overlay() {
        let mut request = generate_request();
        request.speaker_label_overrides = vec![SpeakerLabelOverride {
            source_speaker: Some("Them".into()),
            replacement: "Alex".into(),
        }];

        assert_eq!(
            generate_command(&request, "models/note.d/m/r").unwrap(),
            format!(
                "{{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.generate\",\"arguments\":{{\"meeting_id\":\"meeting-a\",\"transcript_id\":\"{}\",\"speaker_label_overrides\":[{{\"source_speaker\":\"Them\",\"replacement\":\"Alex\"}}],\"model_directory\":\"models/note.d/m/r\",\"deadline_s\":3600}}}}\n",
                "c".repeat(64)
            )
            .into_bytes()
        );
    }

    #[test]
    fn generate_command_carries_source_bound_vocabulary_replacements() {
        let mut request = generate_request();
        request.vocabulary_replacements = vec![VocabularyRangeReplacement {
            turn: 2,
            char_start: 4,
            char_end: 10,
            source_sha256: "d".repeat(64),
            replacement: "Kibble".into(),
        }];

        assert_eq!(
            generate_command(&request, "models/note.d/m/r").unwrap(),
            format!(
                "{{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.generate\",\"arguments\":{{\"meeting_id\":\"meeting-a\",\"transcript_id\":\"{}\",\"vocabulary_replacements\":[{{\"turn\":2,\"char_start\":4,\"char_end\":10,\"source_sha256\":\"{}\",\"replacement\":\"Kibble\"}}],\"model_directory\":\"models/note.d/m/r\",\"deadline_s\":3600}}}}\n",
                "c".repeat(64),
                "d".repeat(64),
            )
            .into_bytes()
        );
    }

    #[test]
    fn generate_command_carries_the_operators_pre_meeting_context() {
        let mut request = generate_request();
        request.pre_meeting_context = Some("Deciding the Q3 roadmap.".into());

        assert_eq!(
            generate_command(&request, "models/note.d/m/r").unwrap(),
            format!(
                "{{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.generate\",\"arguments\":{{\"meeting_id\":\"meeting-a\",\"transcript_id\":\"{}\",\"pre_meeting_context\":\"Deciding the Q3 roadmap.\",\"model_directory\":\"models/note.d/m/r\",\"deadline_s\":3600}}}}\n",
                "c".repeat(64)
            )
            .into_bytes()
        );
    }

    /// Decision 6's gate at this layer: a request with no pre-meeting context
    /// must still produce exactly the frame `generate_command_is_closed_and_bounded`
    /// pins, with no `pre_meeting_context` key at all.
    #[test]
    fn absent_pre_meeting_context_leaves_the_generate_command_unchanged() {
        let request = generate_request();
        assert!(request.pre_meeting_context.is_none());
        assert_eq!(
            generate_command(&request, "models/note.d/m/r").unwrap(),
            format!(
                "{{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.generate\",\"arguments\":{{\"meeting_id\":\"meeting-a\",\"transcript_id\":\"{}\",\"model_directory\":\"models/note.d/m/r\",\"deadline_s\":3600}}}}\n",
                "c".repeat(64)
            )
            .into_bytes()
        );
    }

    #[test]
    fn malformed_or_oversized_pre_meeting_context_refuses_before_launch() {
        let fixture = generative_fixture("descriptor-binding");
        let generator = note_generator(&fixture, 2_000, 2_000);
        for invalid in [Some(String::new()), Some("x".repeat(16 * 1024 + 1))] {
            let mut malformed = generate_request();
            malformed.pre_meeting_context = invalid;
            assert_eq!(
                generator.generate(&malformed),
                Err(ProjectTransportError::Unavailable)
            );
        }
        assert!(!fixture.storage_root.join("bound-command").exists());
    }

    #[test]
    fn malformed_or_oversized_vocabulary_replacements_refuse_before_launch() {
        let fixture = generative_fixture("descriptor-binding");
        let generator = note_generator(&fixture, 2_000, 2_000);
        let mut malformed = generate_request();
        malformed.vocabulary_replacements = vec![VocabularyRangeReplacement {
            turn: 0,
            char_start: 4,
            char_end: 4,
            source_sha256: "d".repeat(64),
            replacement: "Kibble".into(),
        }];
        assert_eq!(
            generator.generate(&malformed),
            Err(ProjectTransportError::Unavailable)
        );
        assert!(!fixture.storage_root.join("bound-command").exists());
    }

    #[test]
    fn staged_descriptors_launch_when_all_final_fd_targets_are_occupied() {
        let fixture = launch_fixture("descriptor-binding");
        let marker = fixture.storage_root.join("bound-command");
        let output = launch_bootstrap(&fixture, None, true);
        assert!(output.status.success());
        assert!(marker.exists());
    }

    #[test]
    fn rust_launch_binds_manifest_bridge_validator_and_command_descriptors() {
        let fixture = launch_fixture("descriptor-binding");
        let result = projector(&fixture, 2_000, 2_000)
            .project(&request())
            .unwrap();
        assert_eq!(result, b"{}\n");
        assert_eq!(
            fs::read(fixture.storage_root.join("bound-command")).unwrap(),
            project_command(&request()).unwrap()
        );
    }

    #[test]
    fn rust_launch_enforces_ready_and_result_timeouts() {
        let ready = launch_fixture("ready-timeout");
        assert_eq!(
            projector(&ready, 50, 2_000).project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
        let result = launch_fixture("result-timeout");
        assert_eq!(
            projector(&result, 2_000, 50).project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
    }

    #[test]
    fn ready_then_exit_is_refused_without_writing_a_command() {
        let fixture = launch_fixture("ready-then-exit");
        assert_eq!(
            projector(&fixture, 2_000, 2_000).project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
        assert!(!fixture.storage_root.join("bound-command").exists());
    }

    #[test]
    fn cancellation_after_result_eof_before_child_exit_never_returns_success() {
        let fixture = launch_fixture("cancel-after-result");
        let marker = fixture.storage_root.join("result-eof");
        let cancellation = ProjectionCancellation::default();
        let child_cancellation = cancellation.clone();
        let projector = projector(&fixture, 2_000, 2_000);
        let worker = std::thread::spawn(move || {
            projector.project_with_cancellation(&request(), &child_cancellation)
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
        cancellation.cancel();
        assert_eq!(
            worker.join().unwrap(),
            Err(ProjectTransportError::Cancelled)
        );
    }

    #[test]
    fn project_command_is_closed_and_bounded() {
        assert_eq!(
            project_command(&request()).unwrap(),
            b"{\"schema\":\"note-bridge-command/1\",\"request_id\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"note.project\",\"arguments\":{\"meeting_id\":\"meeting-a\",\"note_id\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"transcript_id\":\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"}}\n"
        );
    }

    #[test]
    fn ready_and_terminal_frames_refuse_extra_or_malformed_protocol_bytes() {
        let digest = "a".repeat(64);
        let ready = format!(
            "{{\"schema\":\"note-bridge-event/1\",\"event\":\"ready\",\"protocol\":1,\"role\":\"project\",\"manifest_sha256\":\"{digest}\",\"operations\":[\"note.project\"]}}\n"
        );
        assert!(parse_ready(ready.as_bytes(), &digest, PROJECT_ROLE).is_ok());
        assert!(parse_ready(ready.as_bytes(), &digest, GENERATE_ROLE).is_err());
        assert!(
            parse_ready(
                format!("{}{{}}\n", ready.trim_end()).as_bytes(),
                &digest,
                PROJECT_ROLE
            )
            .is_err()
        );
        let generate_ready = format!(
            "{{\"schema\":\"note-bridge-event/1\",\"event\":\"ready\",\"protocol\":1,\"role\":\"generate\",\"manifest_sha256\":\"{digest}\",\"operations\":[\"note.generate\"]}}\n"
        );
        assert!(parse_ready(generate_ready.as_bytes(), &digest, GENERATE_ROLE).is_ok());
        assert!(parse_ready(generate_ready.as_bytes(), &digest, PROJECT_ROLE).is_err());
        assert!(read_to_exact_eof(&b"{}\n{}\n"[..]).is_err());
        assert!(read_to_exact_eof(&vec![b'x'; MAX_PROJECTION_FRAME_BYTES + 1][..]).is_err());
    }

    #[test]
    fn security_framework_live_pid_binding_matches_the_current_sha256_code_directory() {
        let current = security_self_code().unwrap();
        security_dynamic_check(current.as_ref(), SECURITY_NO_NETWORK_ACCESS, None).unwrap();
        let bound = security_live_code(std::process::id()).unwrap();
        security_dynamic_check(bound.as_ref(), SECURITY_NO_NETWORK_ACCESS, None).unwrap();
        let current_information = security_signing_information(current.as_ref(), true).unwrap();
        let bound_information = security_signing_information(bound.as_ref(), true).unwrap();
        assert_eq!(
            sha256_code_directory_identity(current_information.as_concrete_TypeRef()).unwrap(),
            sha256_code_directory_identity(bound_information.as_concrete_TypeRef()).unwrap(),
        );
    }

    #[test]
    fn system_process_start_identity_is_stable_at_microsecond_and_absolute_time_precision() {
        let inspector = SystemProcessStartTimeInspector;
        let first = inspector.start_time(std::process::id()).unwrap().unwrap();
        let second = inspector.start_time(std::process::id()).unwrap().unwrap();
        assert!(first.is_valid());
        assert_eq!(first, second);
    }

    fn injected_outer_identity() -> OuterCodeIdentity {
        OuterCodeIdentity {
            bundle_path: PathBuf::from("/Applications/Local Meeting Notes.app"),
            main_executable: PathBuf::from(
                "/Applications/Local Meeting Notes.app/Contents/MacOS/local-meeting-notes-desktop",
            ),
            team_identifier: "TEAM123456".into(),
            signing_identifier: "com.ninochavez.local-meeting-notes".into(),
        }
    }

    fn injected_runtime_identity() -> CodeIdentity {
        CodeIdentity {
            path: PathBuf::from(
                "/Applications/Local Meeting Notes.app/Contents/Resources/python-runtime/bin/python3.12",
            ),
            main_executable: PathBuf::from(
                "/Applications/Local Meeting Notes.app/Contents/Resources/python-runtime/bin/python3.12",
            ),
            designated_requirement: vec![0xfa, 0xde, 0x0c, 0x00],
            sha256_code_directory_identity: [0x51; 20],
            team_identifier: "TEAM123456".into(),
            signing_identifier: "com.ninochavez.local-meeting-notes.python-runtime".into(),
            signature_flags: CODE_SIGNATURE_RUNTIME | 0x100,
            python_entitlements_exact: true,
        }
    }

    #[test]
    fn injected_transient_path_swap_and_restore_cannot_reconcile_static_and_live_code() {
        let outer = injected_outer_identity();
        let live = injected_runtime_identity();
        let mut admitted_during_swap = live.clone();
        admitted_during_swap.sha256_code_directory_identity = [0x52; 20];
        admitted_during_swap.designated_requirement = vec![0xfa, 0xde, 0x0c, 0x01];
        assert_eq!(admitted_during_swap.path, live.path);
        assert_eq!(
            verify_runtime_identity_pair(&admitted_during_swap, &live, &live.path, &outer),
            Err(InternalOutcome::Unavailable)
        );
    }

    #[test]
    fn injected_wrong_same_team_bundle_is_not_admitted_by_team_alone() {
        let outer = injected_outer_identity();
        let mut wrong = injected_runtime_identity();
        wrong.signing_identifier = "com.ninochavez.other-app.python-runtime".into();
        let expected_path = wrong.path.clone();
        assert_eq!(wrong.team_identifier, outer.team_identifier);
        assert_eq!(
            verify_runtime_identity_pair(&wrong, &wrong, &expected_path, &outer),
            Err(InternalOutcome::Unavailable)
        );
    }

    #[test]
    fn injected_runtime_word_cannot_substitute_for_the_hardened_runtime_bit() {
        let outer = injected_outer_identity();
        let mut identity = injected_runtime_identity();
        identity.designated_requirement = b"identifier contains runtime".to_vec();
        identity.signature_flags &= !CODE_SIGNATURE_RUNTIME;
        let expected_path = identity.path.clone();
        assert_eq!(
            verify_runtime_identity_pair(&identity, &identity, &expected_path, &outer),
            Err(InternalOutcome::Unavailable)
        );
    }

    #[test]
    fn injected_entitlement_drift_between_static_and_live_code_is_refused() {
        let outer = injected_outer_identity();
        let static_identity = injected_runtime_identity();
        let mut live_identity = static_identity.clone();
        live_identity.python_entitlements_exact = false;
        assert_eq!(
            verify_runtime_identity_pair(
                &static_identity,
                &live_identity,
                &static_identity.path,
                &outer,
            ),
            Err(InternalOutcome::Unavailable)
        );
    }

    #[test]
    fn injected_dynamic_identity_mismatch_is_refused_before_authority_is_sent() {
        let outer = injected_outer_identity();
        let static_identity = injected_runtime_identity();
        let mut live_identity = static_identity.clone();
        live_identity.designated_requirement = vec![0xfa, 0xde, 0x0c, 0xff];
        assert_eq!(
            verify_runtime_identity_pair(
                &static_identity,
                &live_identity,
                &static_identity.path,
                &outer,
            ),
            Err(InternalOutcome::Unavailable)
        );
    }

    #[test]
    fn persistent_byte_identical_runtime_replacement_is_refused_on_file_identity() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let path = root.join("python3.12");
        let replacement = root.join("replacement");
        private_file(&path, b"same signed runtime bytes", true);
        private_file(&replacement, b"same signed runtime bytes", true);
        let file = File::open(&path).unwrap();
        let identity = FileIdentity::from_file(&file).unwrap();
        let digest = digest_file(&file, identity).unwrap();
        let pinned = PinnedResource {
            file,
            path: path.clone(),
            identity,
            digest: digest.clone(),
        };
        fs::rename(&replacement, &path).unwrap();

        let mut static_identity = injected_runtime_identity();
        static_identity.path = path.clone();
        static_identity.main_executable = path.clone();
        let live_identity = static_identity.clone();
        assert!(
            verify_runtime_identity_pair(
                &static_identity,
                &live_identity,
                &path,
                &injected_outer_identity(),
            )
            .is_ok()
        );
        let live_file = open_absolute_file(&path).unwrap();
        let live_file_identity = FileIdentity::from_file(&live_file).unwrap();
        assert!(live_file_identity != pinned.identity);
        assert_eq!(digest_file(&live_file, live_file_identity).unwrap(), digest);
        assert_eq!(
            verify_live_executable_file(&live_identity.main_executable, &pinned),
            Err(InternalOutcome::Unavailable)
        );
    }

    struct FakeLiveCode;

    impl RetainedLiveCode for FakeLiveCode {
        fn require_same_executable(&self, _: &PinnedResource) -> Result<(), InternalOutcome> {
            Ok(())
        }
    }

    struct FakePreparedAdmission;

    impl PreparedCodeAdmission for FakePreparedAdmission {
        fn bind_live(
            &self,
            _: u32,
            _: &PinnedResource,
        ) -> Result<Box<dyn RetainedLiveCode>, InternalOutcome> {
            Ok(Box::new(FakeLiveCode))
        }
    }

    struct InjectedStartTimes(Mutex<VecDeque<ProcessStartTime>>);

    impl ProcessStartTimeInspector for InjectedStartTimes {
        fn start_time(&self, _: u32) -> io::Result<Option<ProcessStartTime>> {
            Ok(self.0.lock().unwrap().pop_front())
        }
    }

    #[test]
    fn injected_start_time_mismatch_refuses_the_retained_live_code_binding() {
        let fixture = launch_fixture("descriptor-binding");
        let runtime = verify_manifest(&fixture.manifest_path, PROJECT_ROLE).unwrap();
        let inspector = InjectedStartTimes(Mutex::new(VecDeque::from([
            ProcessStartTime {
                epoch_seconds: 100,
                microseconds: 1,
                absolute_time: 10,
            },
            ProcessStartTime {
                epoch_seconds: 100,
                microseconds: 2,
                absolute_time: 10,
            },
        ])));
        assert!(matches!(
            bind_live_code(
                &FakePreparedAdmission,
                4242,
                &inspector,
                &runtime.executable,
            ),
            Err(InternalOutcome::Unavailable)
        ));
    }

    #[test]
    fn cancellation_kills_the_owned_process_group_before_returning() {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("trap '' TERM; while :; do :; done")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
        let mut child = command.spawn().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut guard = ChildGuard::new(child, StderrMonitor::start(stderr));
        let cancellation = ProjectionCancellation::default();
        cancellation.cancel();
        let (_, receiver) = mpsc::sync_channel::<()>(1);
        assert_eq!(
            wait_receiver(
                &receiver,
                Instant::now() + Duration::from_secs(1),
                &cancellation,
                &mut guard
            ),
            Err(InternalOutcome::Cancelled)
        );
        assert!(!group_exists(guard.process_group_id));
    }

    #[test]
    fn product_admission_refuses_manifest_resources_outside_the_signed_resource_root() {
        let temporary = TempDir::new().unwrap();
        let root = temporary
            .path()
            .join("LocalMeetingNotes.app/Contents/Resources");
        fs::create_dir_all(&root).unwrap();
        let outside = temporary.path().join("other/bridge");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(root.join("manifest"), b"manifest").unwrap();
        fs::write(outside, b"bridge").unwrap();
        for path in [root.join("manifest"), temporary.path().join("other/bridge")] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let directory = File::open(temporary.path()).unwrap();
        let manifest = PinnedResource::open(
            &directory,
            temporary.path(),
            &RuntimeManifestResource {
                relative_path: "LocalMeetingNotes.app/Contents/Resources/manifest".into(),
                sha256: format!("{:x}", Sha256::digest(b"manifest")),
            },
        )
        .unwrap();
        let bridge = PinnedResource::open(
            &directory,
            temporary.path(),
            &RuntimeManifestResource {
                relative_path: "other/bridge".into(),
                sha256: format!("{:x}", Sha256::digest(b"bridge")),
            },
        )
        .unwrap();
        assert!(!resources_are_inside(&root, [&manifest, &bridge]));
    }
    #[test]
    fn canonical_manifest_bytes_match_python_canonical_json_for_both_roles() {
        let fixture = launch_fixture("descriptor-binding");
        let [runtime, bridge, validator] = fixture.manifest_resources();
        let generator = fixture.generator();
        for (role, generator, models) in [
            (PROJECT_ROLE, None, Vec::new()),
            (GENERATE_ROLE, Some(&generator), fixture.pinned_models()),
        ] {
            let rendered =
                canonical_manifest(role, &runtime, &bridge, &validator, generator, &models)
                    .into_bytes();
            assert_eq!(
                python_canonical(&rendered),
                rendered,
                "Rust and Python must agree on the one canonical manifest byte sequence"
            );
        }
    }

    #[test]
    fn verify_manifest_pins_a_generator_and_its_models_on_the_generate_role() {
        let fixture = generative_fixture("descriptor-binding");
        let runtime = verify_manifest(&fixture.generate_manifest_path, GENERATE_ROLE).unwrap();
        let generator = runtime.generator.expect("generator-bearing manifest");
        assert_eq!(
            generator.digest,
            format!("{:x}", Sha256::digest(GENERATOR_FIXTURE))
        );
        assert_eq!(generator.path, fixture.resources.join("note-generator.py"));
        assert_eq!(
            runtime
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["note-generator-config", "note-generator-weights"]
        );
    }

    /// The role and the generator move together in both directions, matching
    /// `_require_generator_admission` in `worker/note_bridge.py`.
    #[test]
    fn verify_manifest_refuses_every_mismatch_of_role_generator_and_models() {
        let fixture = launch_fixture("descriptor-binding");
        let generator = fixture.generator();
        let pinned = fixture.pinned_models();
        let path = &fixture.generate_manifest_path;
        for (role, generator, models) in [
            (GENERATE_ROLE, Some(&generator), Vec::new()),
            (GENERATE_ROLE, None, pinned.clone()),
            (GENERATE_ROLE, None, Vec::new()),
            (PROJECT_ROLE, Some(&generator), pinned.clone()),
        ] {
            fixture.write_manifest(path, role, generator, &models);
            assert!(verify_manifest(path, role).is_err());
        }
        fixture.write_manifest(path, PROJECT_ROLE, None, &[]);
        assert!(verify_manifest(path, GENERATE_ROLE).is_err());
        assert!(verify_manifest(path, PROJECT_ROLE).is_ok());
    }

    /// The canonicality rule has to be tested by breaking it, because every
    /// manifest these tests write is produced by `canonical_manifest` and is
    /// therefore canonical by construction. A blind mutation that deleted the
    /// byte comparison outright survived the whole suite.
    ///
    /// This matters more than an ordinary missing case. Rust accepting a
    /// non-canonical manifest that `worker/note_bridge.py` then refuses is a
    /// cross-language divergence: the admission would succeed here and the
    /// child would refuse at startup, reported as a bare transport failure.
    #[test]
    fn verify_manifest_refuses_a_semantically_identical_noncanonical_manifest() {
        let fixture = launch_fixture("descriptor-binding");
        let canonical = fs::read_to_string(&fixture.manifest_path).unwrap();
        assert!(verify_manifest(&fixture.manifest_path, PROJECT_ROLE).is_ok());

        // Same JSON, same fields, same order, same digests -- only the
        // separator spelling differs, which is what `json.dumps` pins.
        let compact = canonical.replace("\": \"", "\":\"");
        assert_ne!(compact, canonical, "the perturbation must change the bytes");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&compact).unwrap(),
            serde_json::from_str::<serde_json::Value>(&canonical).unwrap(),
            "and must not change the document"
        );
        private_file(&fixture.manifest_path, compact.as_bytes(), false);
        assert!(verify_manifest(&fixture.manifest_path, PROJECT_ROLE).is_err());
    }

    /// The identifier bound is 128 characters. Nothing exercised it until a
    /// blind mutation widened it to 4096 and the suite could not tell.
    #[test]
    fn verify_manifest_bounds_the_model_identifier_length() {
        let fixture = launch_fixture("descriptor-binding");
        let generator = fixture.generator();
        let digest = fixture.pinned_models()[0].sha256.clone();
        let path = &fixture.generate_manifest_path;

        let at_bound = vec![RuntimeManifestModel {
            id: "n".repeat(128),
            sha256: digest.clone(),
        }];
        fixture.write_manifest(path, GENERATE_ROLE, Some(&generator), &at_bound);
        assert!(
            verify_manifest(path, GENERATE_ROLE).is_ok(),
            "128 characters is inside the bound"
        );

        let past_bound = vec![RuntimeManifestModel {
            id: "n".repeat(129),
            sha256: digest,
        }];
        fixture.write_manifest(path, GENERATE_ROLE, Some(&generator), &past_bound);
        assert!(verify_manifest(path, GENERATE_ROLE).is_err());
    }

    /// An identifier is a name, not a path. This is the narrower-than-the-
    /// bridge rule I asserted to the worker side as a contract, and until a
    /// second mutation round widened the charset to admit `.` and `/`, the
    /// only test touching it used a space -- a character both sides reject.
    /// The claim was untested at exactly the point where the two sides differ.
    #[test]
    fn verify_manifest_refuses_path_shaped_model_identifiers() {
        let fixture = launch_fixture("descriptor-binding");
        let generator = fixture.generator();
        let digest = fixture.pinned_models()[0].sha256.clone();
        let path = &fixture.generate_manifest_path;
        for identifier in [
            "note.generator",
            "note/generator",
            "../generator",
            "note generator",
        ] {
            let models = vec![RuntimeManifestModel {
                id: identifier.into(),
                sha256: digest.clone(),
            }];
            fixture.write_manifest(path, GENERATE_ROLE, Some(&generator), &models);
            assert!(
                verify_manifest(path, GENERATE_ROLE).is_err(),
                "{identifier} is not a name"
            );
        }
        let models = vec![RuntimeManifestModel {
            id: "note-generator_weights2".into(),
            sha256: digest,
        }];
        fixture.write_manifest(path, GENERATE_ROLE, Some(&generator), &models);
        assert!(
            verify_manifest(path, GENERATE_ROLE).is_ok(),
            "the bound must not be so tight it rejects a plain name"
        );
    }

    /// A pinned set that two catalog entries both provide names no single
    /// model, so admission refuses rather than picking one. The fixture
    /// carries a single model, so nothing could exercise this until a
    /// mutation deleted the check and the suite could not tell.
    #[test]
    fn admission_refuses_a_pinned_set_two_catalog_entries_both_provide() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());

        let mut ambiguous = fixture.catalog.clone();
        let mut twin = ambiguous.note_models[0].clone();
        twin.id = "note-generator-twin".into();
        twin.revision = "c".repeat(40);
        for file in &mut twin.files {
            file.url = format!("https://example.test/{}/{}", twin.revision, file.name);
        }
        ambiguous.note_models.push(twin);
        ambiguous.validate().expect("two entries may share digests");

        assert!(
            admitted_process_projector(
                &fixture.storage,
                &ambiguous,
                &fixture.manifest_path,
                &fixture.generate_manifest_path,
            )
            .is_none(),
            "an ambiguous pin is refused, never resolved by position"
        );
    }

    /// Admission verifies the manifest it will actually hand the child, not
    /// only the one it read the pin from. A verified generator and a verified
    /// model do not authorize a project manifest that no longer checks out.
    #[test]
    fn admission_refuses_when_the_project_manifest_does_not_verify() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());
        fs::remove_file(&fixture.manifest_path).unwrap();
        assert!(!fixture.is_admitted());
        assert_eq!(
            fixture.admit().project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
    }

    /// A manifest resource path may not climb out of the resource root. The
    /// charset alone admits `..`, so the component check is the whole of the
    /// rule; the target here is a real file outside the root, so the test
    /// witnesses that check rather than a failed open.
    #[test]
    fn verify_manifest_refuses_a_resource_path_that_climbs_out_of_the_root() {
        let fixture = launch_fixture("descriptor-binding");
        let outside = fixture.resources.parent().unwrap().join("note-bridge.py");
        private_file(&outside, LAUNCH_FIXTURE_BRIDGE.as_bytes(), false);

        let [runtime, _, validator] = fixture.manifest_resources();
        let climbing = RuntimeManifestResource {
            relative_path: "../note-bridge.py".into(),
            sha256: format!("{:x}", Sha256::digest(LAUNCH_FIXTURE_BRIDGE.as_bytes())),
        };
        private_file(
            &fixture.manifest_path,
            canonical_manifest(PROJECT_ROLE, &runtime, &climbing, &validator, None, &[]).as_bytes(),
            false,
        );
        assert!(verify_manifest(&fixture.manifest_path, PROJECT_ROLE).is_err());
    }

    #[test]
    fn verify_manifest_refuses_repeated_and_malformed_model_entries() {
        let fixture = launch_fixture("descriptor-binding");
        let generator = fixture.generator();
        let pinned = fixture.pinned_models();
        let path = &fixture.generate_manifest_path;
        // Uniqueness is two rules, and a case that repeats both witnesses
        // neither: the digest clause refuses it and the identifier clause is
        // never reached. Each repetition needs the other field distinct.
        let repeated_identifier = vec![
            pinned[0].clone(),
            RuntimeManifestModel {
                id: pinned[0].id.clone(),
                sha256: pinned[1].sha256.clone(),
            },
        ];
        let repeated_digest = vec![
            pinned[0].clone(),
            RuntimeManifestModel {
                id: pinned[1].id.clone(),
                sha256: pinned[0].sha256.clone(),
            },
        ];
        for models in [
            vec![pinned[0].clone(), pinned[0].clone()],
            repeated_identifier,
            repeated_digest,
            vec![RuntimeManifestModel {
                id: "note generator".into(),
                sha256: pinned[0].sha256.clone(),
            }],
            vec![RuntimeManifestModel {
                id: pinned[0].id.clone(),
                sha256: "abc".into(),
            }],
        ] {
            fixture.write_manifest(path, GENERATE_ROLE, Some(&generator), &models);
            assert!(verify_manifest(path, GENERATE_ROLE).is_err());
        }
    }

    /// `note_runtime_models` names every file of a sharded MLX tree with a
    /// distinct identifier — the widening `docs/note-runtime-decision.md`
    /// seam 3 recorded as blocked on the catalog's note-model role. The
    /// expected list below is the cross-language contract:
    /// `worker/build_manifest.py::note_runtime_model_id` must derive exactly
    /// these identifiers for the same entry, because admission compares the
    /// manifest Python writes against the derivation Rust performs here.
    #[test]
    fn note_runtime_model_ids_name_every_file_of_a_sharded_model_distinctly() {
        let revision = "c".repeat(40);
        let file = |role, name: &str| NoteModelFile {
            role,
            name: name.into(),
            url: format!("https://example.test/{revision}/{name}"),
            bytes: 1,
            sha256: format!("{:x}", Sha256::digest(name.as_bytes())),
        };
        let sharded = NoteModel {
            id: "note-generator-sharded-test".into(),
            revision: revision.clone(),
            title: "Sharded note generator".into(),
            detail: "A sharded local note-generation model.".into(),
            download_bytes: 6,
            installed_bytes: 6,
            files: vec![
                file(NoteModelFileRole::Config, "config.json"),
                file(NoteModelFileRole::Weights, "model-00001-of-00002.safetensors"),
                file(NoteModelFileRole::Weights, "model-00002-of-00002.safetensors"),
                file(NoteModelFileRole::WeightsIndex, "model.safetensors.index.json"),
                file(NoteModelFileRole::Tokenizer, "tokenizer.json"),
                file(NoteModelFileRole::TokenizerConfig, "tokenizer_config.json"),
            ],
        };
        ModelCatalog {
            schema: ModelCatalogSchema::V1,
            models: vec![transcript_fixture_model()],
            note_models: vec![sharded.clone()],
        }
        .validate()
        .unwrap();
        assert_eq!(
            note_runtime_models(&sharded)
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            [
                "note-generator-config",
                "note-generator-tokenizer",
                "note-generator-tokenizer-config",
                "note-generator-weights-00001-of-00002",
                "note-generator-weights-00002-of-00002",
                "note-generator-weights-index",
            ]
        );

        // A weights name outside the shard shape that collides with another
        // file's identifier collapses the derivation to empty — admission can
        // then never match a non-empty pinned set, an honest refusal instead
        // of an ambiguous one. Such an entry cannot come from `validate()`d
        // catalogs today, but the guard is what makes the mapping injective
        // by construction rather than by inheritance.
        let mut collided = sharded.clone();
        collided.files[1].name = "model-00002-of-00002.extra.safetensors".into();
        collided.files[1].sha256 = "e".repeat(64);
        assert!(
            note_runtime_models(&collided).is_empty()
                || note_runtime_models(&collided)
                    .iter()
                    .map(|model| model.id.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == note_runtime_models(&collided).len(),
            "duplicate identifiers must never survive the derivation"
        );
        let mut duplicate = sharded.clone();
        duplicate.files[2].name = "model-00001-of-00002.safetensors".into();
        assert!(
            note_runtime_models(&duplicate).is_empty(),
            "an identifier collision collapses the derivation to empty"
        );
    }

    #[test]
    fn admission_refuses_the_packaged_validator_only_manifest_exactly_as_the_default_does() {
        let fixture = launch_fixture("descriptor-binding");
        fixture.install_note_model();
        assert!(!fixture.is_admitted());
        assert_eq!(
            fixture.admit().project(&request()),
            UnavailableProjector.project(&request())
        );
    }

    #[test]
    fn admission_refuses_a_generator_whose_model_is_not_installed() {
        let fixture = launch_fixture("descriptor-binding");
        fixture.write_generate_manifest(Some(&fixture.generator()), &fixture.pinned_models());
        assert!(!fixture.is_admitted());
        assert_eq!(
            fixture.admit().project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
    }

    #[test]
    fn admission_refuses_an_installed_model_whose_bytes_or_inventory_changed() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());
        let weights = fixture.model_directory().join("model.safetensors");
        fs::write(&weights, b"different note weights of a different length").unwrap();
        assert!(!fixture.is_admitted());
        fs::write(&weights, NOTE_WEIGHTS_FIXTURE).unwrap();
        assert!(fixture.is_admitted());
        fs::write(fixture.model_directory().join("extra.bin"), b"extra").unwrap();
        assert!(!fixture.is_admitted());
    }

    #[test]
    fn admission_refuses_pinned_models_no_catalog_entry_provides() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());
        let mut foreign = fixture.pinned_models();
        foreign[1].sha256 = "0".repeat(64);
        fixture.write_generate_manifest(Some(&fixture.generator()), &foreign);
        assert!(!fixture.is_admitted());
    }

    #[test]
    fn admission_refuses_a_missing_or_tampered_generator_resource() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());
        private_file(
            &fixture.resources.join("note-generator.py"),
            b"tampered generator",
            false,
        );
        assert!(!fixture.is_admitted());
        fs::remove_file(fixture.resources.join("note-generator.py")).unwrap();
        assert!(!fixture.is_admitted());
        assert_eq!(
            fixture.admit().project(&request()),
            Err(ProjectTransportError::Unavailable)
        );
    }

    #[test]
    fn an_admitted_projector_still_drives_the_child_on_the_project_manifest() {
        let fixture = generative_fixture("descriptor-binding");
        assert!(fixture.is_admitted());
        let result = projector(&fixture, 2_000, 2_000)
            .project(&request())
            .unwrap();
        assert_eq!(result, b"{}\n");
        assert_eq!(
            fs::read(fixture.storage_root.join("bound-command")).unwrap(),
            project_command(&request()).unwrap()
        );
    }

    #[test]
    fn an_admitted_projector_honors_cancellation_before_the_child_is_launched() {
        let fixture = generative_fixture("descriptor-binding");
        let cancellation = ProjectionCancellation::default();
        cancellation.cancel();
        assert_eq!(
            projector(&fixture, 2_000, 2_000).project_with_cancellation(&request(), &cancellation),
            Err(ProjectTransportError::Cancelled)
        );
        assert!(!fixture.storage_root.join("bound-command").exists());
    }

    fn generation_frame(outcome: &str, generation: serde_json::Value, failure: serde_json::Value) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "note-generation-result/1",
            "request_id": "11111111-1111-4111-8111-111111111111",
            "operation": "note.generate",
            "outcome": outcome,
            "generation": generation,
            "failure": failure,
        }))
        .unwrap();
        bytes.push(b'\n');
        bytes
    }

    fn generation_payload() -> serde_json::Value {
        serde_json::json!({
            "schema": "note-generation/1",
            "transcript_sha256": "c".repeat(64),
            "manifest_sha256": "d".repeat(64),
            "candidates": 7,
            "points": [{"point_ordinal": 0, "candidate_id": "cf-a", "evidence_state": "located", "locators": []}],
            "receipt": {"responses": 4, "response_bytes": 100, "last_response_sha256": "e".repeat(64), "elapsed_s": 1.5},
        })
    }

    #[test]
    fn a_generated_frame_parses_and_carries_the_generation_verbatim() {
        let frame = generation_frame("generated", generation_payload(), serde_json::Value::Null);
        assert_eq!(
            parse_note_generation_result(&frame, &generate_request()),
            Ok(NoteGenerationChildOutcome::Generated(generation_payload()))
        );
    }

    #[test]
    fn a_transcript_only_frame_parses_to_its_failure_code() {
        let frame = generation_frame(
            "transcript-only",
            serde_json::Value::Null,
            serde_json::json!({"code": "no-model-candidates", "recoverable": true, "receipt": {}}),
        );
        assert_eq!(
            parse_note_generation_result(&frame, &generate_request()),
            Ok(NoteGenerationChildOutcome::TranscriptOnly {
                code: "no-model-candidates".into(),
                recoverable: true,
            })
        );
    }

    #[test]
    fn generation_result_envelope_violations_are_unavailable() {
        let request = generate_request();
        let ok = generation_frame("generated", generation_payload(), serde_json::Value::Null);
        assert!(parse_note_generation_result(&ok, &request).is_ok());

        // A frame answering a different request.
        let mut foreign = request.clone();
        foreign.request_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        assert_eq!(
            parse_note_generation_result(&ok, &foreign),
            Err(ProjectTransportError::Unavailable)
        );

        // A generation pinned to a different transcript.
        let mut other = request.clone();
        other.transcript_sha256 = "f".repeat(64);
        assert_eq!(
            parse_note_generation_result(&ok, &other),
            Err(ProjectTransportError::Unavailable)
        );

        // Outcome/payload disagreements and malformed envelopes.
        for frame in [
            generation_frame("generated", generation_payload(), serde_json::json!({"code": "x", "recoverable": false, "receipt": {}})),
            generation_frame("generated", serde_json::Value::Null, serde_json::Value::Null),
            generation_frame("transcript-only", generation_payload(), serde_json::Value::Null),
            generation_frame("transcript-only", serde_json::Value::Null, serde_json::Value::Null),
            generation_frame("transcript-only", serde_json::Value::Null, serde_json::json!({"code": "", "recoverable": true, "receipt": {}})),
            generation_frame("transcript-only", serde_json::Value::Null, serde_json::json!({"code": "x", "recoverable": true})),
            generation_frame("refused", serde_json::Value::Null, serde_json::Value::Null),
            ok[..ok.len() - 1].to_vec(),
            b"not json\n".to_vec(),
            Vec::new(),
        ] {
            assert_eq!(
                parse_note_generation_result(&frame, &request),
                Err(ProjectTransportError::Unavailable),
                "frame was accepted: {:?}",
                String::from_utf8_lossy(&frame)
            );
        }

        // A schema or operation swap refuses.
        for (key, value) in [("schema", "note-projection-result/1"), ("operation", "note.project")] {
            let mut root: serde_json::Value = serde_json::from_slice(&ok[..ok.len() - 1]).unwrap();
            root[key] = serde_json::Value::String(value.into());
            let mut bytes = serde_json::to_vec(&root).unwrap();
            bytes.push(b'\n');
            assert_eq!(
                parse_note_generation_result(&bytes, &request),
                Err(ProjectTransportError::Unavailable)
            );
        }
    }
}
