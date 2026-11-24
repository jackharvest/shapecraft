//! What's new: the home screen's row of cards, one per feature a release added (newest first), as the reference
//! shows its own (s30-011: a dark card, a white title, a picture of the feature). Each card names the version
//! that brought the feature, and links to the changelog for every version's notes.
//!
//! Adding a release: put its features at the top, with a picture in `assets/home/` (our own renders or
//! screenshots, 340 × 438, see-through around the model) and a row in ATTRIBUTION.md if it's a new kind of file.

/// Every version's notes.
pub const NOTES_URL: &str = "https://github.com/jackharvest/shapecraft/blob/main/CHANGELOG.md";

pub struct Entry {
    pub version: &'static str,
    pub title: &'static str,
    /// What the detail card says.
    pub text: &'static str,
    /// The picture's name in `assets/home/` (without `.png`).
    pub art: &'static str,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        version: "0.12.9",
        title: "Tutorials for everyone",
        text: "Nine short courses on things from around the house. Start with a coaster, a pencil cup and a phone stand, \
               then learn one tool at a time: Revolve, Loft, Sweep, Pattern, Mirror and Thread. An animated pointer shows \
               every move, everything can be done by clicking, and you can keep what you make as a project.",
        art: "new-tutorials",
    },
    Entry {
        version: "0.10.0",
        title: "Threads",
        text: "Pick a hole or a cylinder and press Thread: it becomes a nut or a bolt in ISO metric, ANSI inch or ACME \
               sizes, right or left hand, along the whole face or part of it. Print Fit gives 3D-printed threads the \
               room they need to turn, and a bolt and its nut always line up.",
        art: "new-threads",
    },
    Entry {
        version: "0.9.0",
        title: "ShapeCraft on iPhone",
        text: "A native iPhone app. On iPhone Duo it's the whole modeler: the front screen holds your projects and \
               opening it up takes you into the model, where fingers move the view and a stylus draws and picks. On \
               other iPhones projects open to view: turn them, change the display, take a screenshot or export. \
               Sideload it onto your own phone (docs/ios.md).",
        art: "new-iphone",
    },
    Entry {
        version: "0.8.0",
        title: "STLs stay light",
        text: "STL and OBJ files come in as mesh bodies, the way they are: in a second, however many triangles. Move, \
               pattern and mirror them, sketch next to them, and cut or join them with Union, Subtract and Intersect. \
               They stay meshes, and projects with them stay small.",
        art: "new-meshes",
    },
    Entry {
        version: "0.5.0",
        title: "Make yourself at home",
        text: "The first launch asks which app's navigation your hands already know, so orbit, pan and zoom work the \
               way you're used to. Change it any time under Preferences ▸ Navigation Presets.",
        art: "new-navigation",
    },
    Entry {
        version: "0.3.0",
        title: "Grab the cube to turn the view",
        text: "Drag the view cube in the top-right corner to orbit freely; it stays where you let go. Clicking a face, \
               edge or corner still jumps to that view, and double-clicking goes back to the default.",
        art: "new-view-cube",
    },
    Entry {
        version: "0.2.0",
        title: "Parameter Cards on canvas",
        text: "Fillet and Split Body show a card with every setting of the step, both while the tool runs and when you \
               edit it from History. Drag the card anywhere; its ⋯ menu decides when it opens.",
        art: "new-cards",
    },
    Entry {
        version: "0.2.0",
        title: "Linked and unlinked copies",
        text: "Copies made with Move or Rotate Around Axis can stay linked to the original, or become their own \
               bodies with the Link badge turned off.",
        art: "new-copies",
    },
    Entry {
        version: "0.2.0",
        title: "Twelve languages",
        text: "Preferences ▸ Language switches the toolbar, sidebars and cards to one of twelve languages.",
        art: "new-languages",
    },
    Entry {
        version: "0.1.0",
        title: "Your .shapr projects open here",
        text: ".shapr files open with their bodies, sketches and steps, and projects you save from ShapeCraft keep \
               their editable history.",
        art: "new-shapr",
    },
];
