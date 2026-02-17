# embedded-3dgfx Documentation

Welcome to the documentation hub for the embedded-3dgfx project!

## 📚 Documentation Index

### Core Documentation
- **[Main README](../README.md)** - Project overview, features, quick start
- **[Physics Examples](../PHYSICS_EXAMPLES.md)** - Rigid body physics demonstrations
- **[Skeletal & Soft Body](../SKELETAL_AND_SOFTBODY.md)** - Animation and deformable objects
- **[Examples README](../examples/README.md)** - Gallery of all 23+ examples

### LVGL Integration (NEW!)

Complete integration package for bringing embedded-3dgfx to LVGL:

#### **[LVGL Integration Summary](LVGL_INTEGRATION_SUMMARY.md)** 🎯 START HERE
Overview of the integration approach with quick start guide, next steps, and community strategy.

**Read this first if you want to:**
- Understand the integration possibilities
- See example code snippets
- Learn about different integration approaches
- Find out how to contribute

#### **[LVGL RFC](LVGL_RFC.md)** 📄 For LVGL Maintainers
Formal Request for Comments proposing embedded-3dgfx as an official LVGL backend.

**Contains:**
- Detailed motivation and problem statement
- Complete technical architecture
- 4-phase implementation plan (12-14 months)
- API examples in C and Rust
- Performance benchmarks and memory analysis
- Success metrics and alternatives considered

**Submit this to:** LVGL Forum or GitHub Discussions for community feedback

#### **[LVGL FFI Analysis](LVGL_FFI_ANALYSIS.md)** 🔧 For Implementers
Deep technical guide for the Foreign Function Interface between Rust and C.

**Covers:**
- Current state of LVGL Rust bindings
- Missing Canvas widget (with full implementation code)
- FFI bridge architecture and data structure mapping
- Memory safety and lifetime management
- Testing strategy and tooling
- 6-8 week implementation roadmap

**Use this if you're:**
- Writing the FFI bridge code
- Adding Canvas widget to lv_binding_rust
- Debugging memory safety issues
- Planning the technical implementation

### Example Code

#### **[LVGL Integration POC](../examples/lvgl_integration_poc.rs)**
Proof of concept example demonstrating 3D rendering with LVGL integration points.

**Run it:**
```bash
cargo run --example lvgl_integration_poc --features std
```

Shows a rotating 3D cube with detailed comments explaining where LVGL APIs would integrate.

---

## 🗺️ Navigation Guide

**I want to...**

✅ **...learn what this project does**
→ Read [Main README](../README.md)

✅ **...see physics demos**
→ Browse [Physics Examples](../PHYSICS_EXAMPLES.md)

✅ **...understand LVGL integration**
→ Start with [Integration Summary](LVGL_INTEGRATION_SUMMARY.md)

✅ **...propose this to LVGL team**
→ Submit the [RFC](LVGL_RFC.md)

✅ **...implement the FFI bridge**
→ Follow [FFI Analysis](LVGL_FFI_ANALYSIS.md)

✅ **...try the demo**
→ Run [LVGL POC example](../examples/lvgl_integration_poc.rs)

✅ **...contribute**
→ Check [Integration Summary § Next Actions](LVGL_INTEGRATION_SUMMARY.md#-next-actions)

---

## 📁 Repository Structure

```
embedded-3dgfx/
├── docs/                          # 👈 You are here
│   ├── README.md                  # This file
│   ├── LVGL_INTEGRATION_SUMMARY.md
│   ├── LVGL_RFC.md
│   └── LVGL_FFI_ANALYSIS.md
│
├── src/                           # Core library code
│   ├── lib.rs                     # Main 3D engine
│   ├── mesh.rs                    # Geometry and rendering
│   ├── physics.rs                 # Rigid body dynamics
│   ├── skeleton.rs                # Skeletal animation
│   └── softbody.rs                # Deformable physics
│
├── examples/                      # 23+ interactive demos
│   ├── rotating_cube.rs           # Basic 3D
│   ├── physics_*.rs               # Physics simulations
│   ├── lvgl_integration_poc.rs    # LVGL integration
│   └── README.md                  # Examples gallery
│
├── README.md                      # Project overview
├── PHYSICS_EXAMPLES.md            # Physics guide
└── SKELETAL_AND_SOFTBODY.md       # Animation guide
```

---

## 🚀 Quick Links

- **GitHub:** https://github.com/leftger/embedded-3dgfx
- **Crates.io:** https://crates.io/crates/embedded-3dgfx
- **LVGL:** https://lvgl.io
- **LVGL Rust Bindings:** https://github.com/lvgl/lv_binding_rust
- **Rust Embedded:** https://docs.rust-embedded.org

---

## 📝 License

MIT OR Apache-2.0 - See [LICENSE](../LICENSE) file for details.

---

**Happy Coding! 🎮**
