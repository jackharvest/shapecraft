// C ABI over the bits of OpenCASCADE that ShapeCraft uses.
//
// Every entry point returns 0 on success or 1 on failure (message in sk_last_error()). No C++ exception
// ever leaves this file.

#include <cmath>
#include <cstdint>
#include <cstring>
#include <exception>
#include <sstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepOffsetAPI_NormalProjection.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <BRepTools.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <optional>
#include <BRepLib_FindSurface.hxx>
#include <BRepAdaptor_CompCurve.hxx>
#include <Geom_Plane.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <LocOpe_DPrism.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <Geom_BSplineCurve.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>
#include <gp_Ax3.hxx>
#include <ElSLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepLib.hxx>
#include <ShapeFix_Solid.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Lin.hxx>
#include <GeomLib_IsPlanarSurface.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <TopTools_ListOfShape.hxx>
#include <gp_Ax1.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GProp_GProps.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <TDocStd_Document.hxx>
#include <TDataStd_Name.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <Quantity_Color.hxx>
#include <TCollection_ExtendedString.hxx>
#include <STEPControl_Writer.hxx>
#include <DESTEP_Parameters.hxx>
#include <StepData_StepModel.hxx>
#include <IGESControl_Reader.hxx>
#include <IGESControl_Writer.hxx>
#include <BRepBuilderAPI_MakeShapeOnMesh.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <BRep_Builder.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

struct SkShape {
  TopoDS_Shape shape;
};

struct SkMesh {
  std::vector<float> positions;
  std::vector<float> normals;
  std::vector<uint32_t> indices;
  std::vector<uint32_t> tri_faces;
  std::vector<std::vector<float>> edges;
  std::vector<uint32_t> edge_ids;
  std::vector<uint32_t> edge_faces;  // two face indices per polyline (UINT32_MAX when missing)
};

namespace {

thread_local std::string g_error;

void set_error(const char* msg) { g_error = msg ? msg : "unknown error"; }

int fail(const char* msg) {
  set_error(msg);
  return 1;
}

double volume_of(const TopoDS_Shape& s) {
  GProp_GProps p;
  BRepGProp::VolumeProperties(s, p);
  return p.Mass();
}

// Some operations "succeed" with nonsense (a shell wall thicker than the part). Reject results that aren't
// valid solids or that didn't remove material when they should have.
// `may_grow`: fillets and chamfers on inside corners (where two merged parts meet) add material, so the result
// may be a little larger than before; anything else that grows the body is nonsense.
int put_checked(SkShape** out, const TopoDS_Shape& result, const TopoDS_Shape& before, const char* what,
                bool may_grow = false) {
  if (result.IsNull()) return fail(what);
  BRepCheck_Analyzer check(result);
  if (!check.IsValid()) return fail(what);
  double v = volume_of(result);
  if (!(v > 1e-9) || v > volume_of(before) * (may_grow ? 1.5 : 1.0 + 1e-9)) return fail(what);
  *out = new SkShape{result};
  return 0;
}

int put(SkShape** out, const TopoDS_Shape& s) {
  if (s.IsNull()) return fail("operation produced an empty shape");
  *out = new SkShape{s};
  return 0;
}

// OCCT logs to stdout by default; the CLI and MCP server use stdout for their own output.
void quiet_messenger() {
  static const bool done = [] {
    Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
    return true;
  }();
  (void)done;
}

template <typename F>
int guard(F&& f) {
  try {
    quiet_messenger();
    return f();
  } catch (const Standard_Failure& e) {
    return fail(e.GetMessageString());
  } catch (const std::exception& e) {
    return fail(e.what());
  } catch (...) {
    return fail("unknown C++ exception");
  }
}

}  // namespace

// For the other translation units (brep.cpp).
int sk_fail_external(const char* msg) { return fail(msg); }
void sk_quiet_external() { quiet_messenger(); }

extern "C" {

const char* sk_last_error() { return g_error.c_str(); }

void sk_shape_free(SkShape* s) { delete s; }

int sk_make_box(double x, double y, double z, double dx, double dy, double dz, SkShape** out) {
  return guard([&] {
    BRepPrimAPI_MakeBox mk(gp_Pnt(x, y, z), dx, dy, dz);
    mk.Build();
    if (!mk.IsDone()) return fail("box failed");
    return put(out, mk.Shape());
  });
}

int sk_extrude_polygon(const double* xyz, size_t n, double dx, double dy, double dz, SkShape** out) {
  return guard([&] {
    if (n < 3) return fail("a polygon needs at least 3 points");
    BRepBuilderAPI_MakePolygon poly;
    for (size_t i = 0; i < n; ++i) poly.Add(gp_Pnt(xyz[3 * i], xyz[3 * i + 1], xyz[3 * i + 2]));
    poly.Close();
    if (!poly.IsDone()) return fail("polygon is degenerate");
    BRepBuilderAPI_MakeFace face(poly.Wire(), Standard_True);
    if (!face.IsDone()) return fail("polygon is not planar");
    BRepPrimAPI_MakePrism prism(face.Face(), gp_Vec(dx, dy, dz));
    prism.Build();
    if (!prism.IsDone()) return fail("extrude failed");
    return put(out, prism.Shape());
  });
}

int sk_extrude_circle(double cx, double cy, double cz, double r, double dx, double dy, double dz, SkShape** out) {
  return guard([&] {
    gp_Vec v(dx, dy, dz);
    if (v.Magnitude() <= 0.0) return fail("zero extrude direction");
    gp_Circ circ(gp_Ax2(gp_Pnt(cx, cy, cz), gp_Dir(v)), r);
    TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(circ);
    TopoDS_Wire wire = BRepBuilderAPI_MakeWire(edge);
    BRepBuilderAPI_MakeFace face(wire, Standard_True);
    if (!face.IsDone()) return fail("circle face failed");
    BRepPrimAPI_MakePrism prism(face.Face(), v);
    prism.Build();
    if (!prism.IsDone()) return fail("extrude failed");
    return put(out, prism.Shape());
  });
}

// op: 0 union, 1 subtract, 2 intersect
int sk_boolean(int op, const SkShape* a, const SkShape* b, SkShape** out) {
  return guard([&] {
    TopoDS_Shape r;
    bool ok = false;
    if (op == 0) {
      BRepAlgoAPI_Fuse f(a->shape, b->shape);
      ok = f.IsDone() && !f.HasErrors();
      if (ok) {
        // Merge faces that ended up on the same surface, so no seams show where parts met.
        f.SimplifyResult();
        r = f.Shape();
      }
    } else if (op == 1) {
      BRepAlgoAPI_Cut c(a->shape, b->shape);
      ok = c.IsDone() && !c.HasErrors();
      if (ok) {
        // Merge faces that ended up on the same surface, so no seams show where parts met.
        c.SimplifyResult();
        r = c.Shape();
      }
    } else if (op == 2) {
      BRepAlgoAPI_Common c(a->shape, b->shape);
      ok = c.IsDone() && !c.HasErrors();
      if (ok) {
        // Merge faces that ended up on the same surface, so no seams show where parts met.
        c.SimplifyResult();
        r = c.Shape();
      }
    } else {
      return fail("unknown boolean op");
    }
    if (!ok) return fail("boolean failed");
    return put(out, r);
  });
}

int sk_fillet_all_edges(const SkShape* s, double radius, SkShape** out) {
  return guard([&] {
    BRepFilletAPI_MakeFillet mk(s->shape);
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    for (int i = 1; i <= edges.Extent(); ++i) mk.Add(radius, TopoDS::Edge(edges(i)));
    mk.Build();
    if (!mk.IsDone()) return fail("fillet failed (radius too large?)");
    return put_checked(out, mk.Shape(), s->shape, "fillet failed (radius too large?)", true);
  });
}

int sk_mass_props(const SkShape* s, double* volume, double* area, double* centroid) {
  return guard([&] {
    GProp_GProps vol, surf;
    // The fixed Gauss rule is exact on faces bounded by lines and conics, but percents off on a round analytic face
    // (cylinder, cone, sphere, torus) bounded by free-form curves (a wrapped profile's hole). A body with such a face
    // is integrated adaptively; everything else keeps the fast rule (the adaptive one is slow on big imports, and
    // unstable on free-form surfaces such as a G2 round).
    bool suspect = false;
    for (TopExp_Explorer fx(s->shape, TopAbs_FACE); fx.More() && !suspect; fx.Next()) {
      const TopoDS_Face& f = TopoDS::Face(fx.Current());
      GeomAbs_SurfaceType st = BRepAdaptor_Surface(f, Standard_False).GetType();
      if (st != GeomAbs_Cylinder && st != GeomAbs_Cone && st != GeomAbs_Sphere && st != GeomAbs_Torus) continue;
      for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More() && !suspect; ex.Next()) {
        GeomAbs_CurveType t = BRepAdaptor_Curve(TopoDS::Edge(ex.Current())).GetType();
        suspect = t == GeomAbs_BSplineCurve || t == GeomAbs_BezierCurve || t == GeomAbs_OtherCurve;
      }
    }
    if (suspect) {
      // 1e-4 relative: the wrapped faces converge to the same volume from 1e-3 down; tighter only costs time.
      BRepGProp::VolumeProperties(s->shape, vol, 1e-4);
      BRepGProp::SurfaceProperties(s->shape, surf, 1e-4);
    } else {
      BRepGProp::VolumeProperties(s->shape, vol);
      BRepGProp::SurfaceProperties(s->shape, surf);
    }
    *volume = vol.Mass();
    *area = surf.Mass();
    // A face or shell has no volume: its middle is the surface's centre of mass.
    gp_Pnt c = std::abs(vol.Mass()) > 1e-12 ? vol.CentreOfMass() : surf.CentreOfMass();
    centroid[0] = c.X();
    centroid[1] = c.Y();
    centroid[2] = c.Z();
    return 0;
  });
}

int sk_bounds(const SkShape* s, double* min, double* max) {
  return guard([&] {
    // Tight bounds from the geometry itself, not padded by tolerances.
    Bnd_Box box;
    BRepBndLib::AddOptimal(s->shape, box, Standard_False, Standard_False);
    if (box.IsVoid()) return fail("shape has no extent");
    box.Get(min[0], min[1], min[2], max[0], max[1], max[2]);
    return 0;
  });
}

int sk_tessellate(const SkShape* s, double deflection, SkMesh** out) {
  return guard([&] {
    BRepMesh_IncrementalMesh(s->shape, deflection, Standard_False, 0.35, Standard_True);
    auto mesh = new SkMesh();
    TopTools_IndexedMapOfShape faces;
    TopExp::MapShapes(s->shape, TopAbs_FACE, faces);
    for (int fi = 1; fi <= faces.Extent(); ++fi) {
      const TopoDS_Face& face = TopoDS::Face(faces(fi));
      TopLoc_Location loc;
      Handle(Poly_Triangulation) tri = BRep_Tool::Triangulation(face, loc);
      if (tri.IsNull()) continue;
      if (!tri->HasNormals()) BRepLib_ToolTriangulatedShape::ComputeNormals(face, tri);
      const bool reversed = face.Orientation() == TopAbs_REVERSED;
      const gp_Trsf trsf = loc.Transformation();
      const uint32_t base = static_cast<uint32_t>(mesh->positions.size() / 3);
      for (int i = 1; i <= tri->NbNodes(); ++i) {
        gp_Pnt p = tri->Node(i).Transformed(trsf);
        mesh->positions.insert(mesh->positions.end(), {float(p.X()), float(p.Y()), float(p.Z())});
        gp_Dir n = tri->HasNormals() ? tri->Normal(i).Transformed(trsf) : gp_Dir(0, 0, 1);
        if (reversed) n.Reverse();
        mesh->normals.insert(mesh->normals.end(), {float(n.X()), float(n.Y()), float(n.Z())});
      }
      for (int i = 1; i <= tri->NbTriangles(); ++i) {
        int a, b, c;
        tri->Triangle(i).Get(a, b, c);
        if (reversed) std::swap(b, c);
        mesh->indices.insert(mesh->indices.end(), {base + uint32_t(a - 1), base + uint32_t(b - 1), base + uint32_t(c - 1)});
        mesh->tri_faces.push_back(uint32_t(fi - 1));
      }
    }
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    TopTools_IndexedMapOfShape face_map;
    TopExp::MapShapes(s->shape, TopAbs_FACE, face_map);
    TopTools_IndexedDataMapOfShapeListOfShape edge_to_faces;
    TopExp::MapShapesAndAncestors(s->shape, TopAbs_EDGE, TopAbs_FACE, edge_to_faces);
    for (int ei = 1; ei <= edges.Extent(); ++ei) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges(ei));
      if (BRep_Tool::Degenerated(edge)) continue;
      uint32_t adj[2] = {UINT32_MAX, UINT32_MAX};
      if (edge_to_faces.Contains(edge)) {
        // A seam (the edge closing a cylinder or sphere on itself) isn't drawn, as in the reference.
        bool seam = false;
        for (TopTools_ListIteratorOfListOfShape it(edge_to_faces.FindFromKey(edge)); it.More(); it.Next())
          seam |= BRep_Tool::IsClosed(edge, TopoDS::Face(it.Value()));
        if (seam) continue;
        int k = 0;
        for (TopTools_ListIteratorOfListOfShape it(edge_to_faces.FindFromKey(edge)); it.More() && k < 2; it.Next()) {
          int fi = face_map.FindIndex(it.Value());
          if (fi > 0 && (k == 0 || adj[0] != uint32_t(fi - 1))) adj[k++] = uint32_t(fi - 1);
        }
      }
      BRepAdaptor_Curve curve(edge);
      GCPnts_TangentialDeflection pts(curve, 0.1, deflection);
      std::vector<float> line;
      for (int i = 1; i <= pts.NbPoints(); ++i) {
        gp_Pnt p = pts.Value(i);
        line.insert(line.end(), {float(p.X()), float(p.Y()), float(p.Z())});
      }
      if (line.size() >= 6) {
        mesh->edges.push_back(std::move(line));
        mesh->edge_ids.push_back(uint32_t(ei - 1));
        mesh->edge_faces.push_back(adj[0]);
        mesh->edge_faces.push_back(adj[1]);
      }
    }
    *out = mesh;
    return 0;
  });
}

void sk_mesh_free(SkMesh* m) { delete m; }
size_t sk_mesh_positions_len(const SkMesh* m) { return m->positions.size(); }
const float* sk_mesh_positions(const SkMesh* m) { return m->positions.data(); }
const float* sk_mesh_normals(const SkMesh* m) { return m->normals.data(); }
size_t sk_mesh_indices_len(const SkMesh* m) { return m->indices.size(); }
const uint32_t* sk_mesh_indices(const SkMesh* m) { return m->indices.data(); }
const uint32_t* sk_mesh_tri_faces(const SkMesh* m) { return m->tri_faces.data(); }
size_t sk_mesh_edge_count(const SkMesh* m) { return m->edges.size(); }
size_t sk_mesh_edge_len(const SkMesh* m, size_t i) { return i < m->edges.size() ? m->edges[i].size() : 0; }
const float* sk_mesh_edge(const SkMesh* m, size_t i) { return i < m->edges.size() ? m->edges[i].data() : nullptr; }
uint32_t sk_mesh_edge_id(const SkMesh* m, size_t i) { return i < m->edge_ids.size() ? m->edge_ids[i] : 0; }
uint32_t sk_mesh_edge_face(const SkMesh* m, size_t i, size_t side) {
  size_t k = i * 2 + (side ? 1 : 0);
  return k < m->edge_faces.size() ? m->edge_faces[k] : UINT32_MAX;
}

// schema: 0 = OCCT's default (AP214), 1 = AP203, 2 = AP214, 3 = AP242.
int sk_write_step_schema(const SkShape* const* shapes, size_t n, const char* path, int schema) {
  return guard([&] {
    DESTEP_Parameters params;
    params.InitFromStatic();
    params.WriteSchema = schema == 1 ? DESTEP_Parameters::WriteMode_StepSchema_AP203
                       : schema == 3 ? DESTEP_Parameters::WriteMode_StepSchema_AP242DIS
                                     : DESTEP_Parameters::WriteMode_StepSchema_AP214IS;
    STEPControl_Writer writer;
    writer.Model(true)->InternalParameters = params;
    for (size_t i = 0; i < n; ++i) {
      if (writer.Transfer(shapes[i]->shape, STEPControl_AsIs, params) != IFSelect_RetDone) return fail("STEP transfer failed");
    }
    if (writer.Write(path) != IFSelect_RetDone) return fail("could not write STEP file");
    return 0;
  });
}

int sk_write_step(const SkShape* const* shapes, size_t n, const char* path) {
  return guard([&] {
    STEPControl_Writer writer;
    for (size_t i = 0; i < n; ++i) {
      if (writer.Transfer(shapes[i]->shape, STEPControl_AsIs) != IFSelect_RetDone) return fail("STEP transfer failed");
    }
    if (writer.Write(path) != IFSelect_RetDone) return fail("could not write STEP file");
    return 0;
  });
}

// Reads a STEP file. Each solid becomes one shape; a file without solids yields its root shape.
int sk_read_step(const char* path, SkShape*** out, size_t* n) {
  return guard([&] {
    STEPControl_Reader reader;
    if (reader.ReadFile(path) != IFSelect_RetDone) return fail("could not read STEP file");
    reader.TransferRoots();
    TopoDS_Shape root = reader.OneShape();
    if (root.IsNull()) return fail("STEP file has no shapes");
    std::vector<TopoDS_Shape> parts;
    for (TopExp_Explorer ex(root, TopAbs_SOLID); ex.More(); ex.Next()) parts.push_back(ex.Current());
    if (parts.empty()) parts.push_back(root);
    auto arr = new SkShape*[parts.size()];
    for (size_t i = 0; i < parts.size(); ++i) arr[i] = new SkShape{parts[i]};
    *out = arr;
    *n = parts.size();
    return 0;
  });
}

// ---- STEP with names, colors and assembly structure (XCAF) ----

thread_local std::string g_meta;

const char* sk_last_meta() { return g_meta.c_str(); }

namespace {

// A label's name as UTF-8, tabs and line breaks turned to spaces (they separate the metadata fields).
std::string label_name(const TDF_Label& l) {
  Handle(TDataStd_Name) n;
  if (l.IsNull() || !l.FindAttribute(TDataStd_Name::GetID(), n)) return std::string();
  const TCollection_ExtendedString& e = n->Get();
  std::string out(static_cast<size_t>(e.LengthOfCString()) + 1, '\0');
  Standard_PCharacter buf = &out[0];
  int len = e.ToUTF8CString(buf);
  out.resize(len > 0 ? static_cast<size_t>(len) : 0);
  for (char& c : out) if (c == '\t' || c == '\n' || c == '\r' || c == '\x1f') c = ' ';
  return out;
}

bool label_color(const Handle(XCAFDoc_ColorTool)& ct, const TDF_Label& l, Quantity_Color& c) {
  if (l.IsNull()) return false;
  return ct->GetColor(l, XCAFDoc_ColorSurf, c) || ct->GetColor(l, XCAFDoc_ColorGen, c);
}

struct Part {
  TopoDS_Shape shape;
  std::string name;
  bool colored;
  Quantity_Color color;
  std::vector<std::string> path;
};

void walk(const Handle(XCAFDoc_ShapeTool)& st, const Handle(XCAFDoc_ColorTool)& ct, const TDF_Label& label, const TopLoc_Location& loc,
          std::vector<std::string> path, bool colored, Quantity_Color color, std::vector<Part>& out, int depth) {
  if (depth > 64 || out.size() > 100000) return;
  TDF_Label ref = label;
  if (st->IsReference(label)) st->GetReferredShape(label, ref);
  std::string name = label_name(label);
  if (name.empty()) name = label_name(ref);
  TopLoc_Location here = loc * st->GetLocation(label);
  Quantity_Color c;
  if (label_color(ct, label, c) || label_color(ct, ref, c)) {
    colored = true;
    color = c;
  }
  if (st->IsAssembly(ref)) {
    TDF_LabelSequence comps;
    st->GetComponents(ref, comps);
    path.push_back(name);
    for (Standard_Integer i = 1; i <= comps.Length(); ++i) walk(st, ct, comps.Value(i), here, path, colored, color, out, depth + 1);
    return;
  }
  TopoDS_Shape shape = st->GetShape(ref);
  if (shape.IsNull()) return;
  std::vector<TopoDS_Shape> solids;
  for (TopExp_Explorer ex(shape, TopAbs_SOLID); ex.More(); ex.Next()) solids.push_back(ex.Current());
  if (solids.empty()) solids.push_back(shape);
  for (size_t k = 0; k < solids.size(); ++k) {
    Part p{solids[k].Moved(here), name, colored, color, path};
    // A solid of its own inside the part may carry its own name and color.
    TDF_Label sub;
    if (solids.size() > 1 && st->FindSubShape(ref, solids[k], sub)) {
      std::string sn = label_name(sub);
      if (!sn.empty()) p.name = sn;
      Quantity_Color sc;
      if (label_color(ct, sub, sc)) {
        p.colored = true;
        p.color = sc;
      }
    }
    if (solids.size() > 1 && p.name == name && !name.empty()) p.name = name + " (" + std::to_string(k + 1) + ")";
    out.push_back(p);
  }
}

}  // namespace

// Reads a STEP file with its product structure: one shape per solid, placed. sk_last_meta() then holds one line
// per shape: name TAB color ("r g b" in sRGB 0..1, or "-") TAB the assemblies it sits in, separated by 0x1F.
int sk_read_step_doc(const char* path, SkShape*** out, size_t* n) {
  return guard([&] {
    g_meta.clear();
    Handle(XCAFApp_Application) app = XCAFApp_Application::GetApplication();
    Handle(TDocStd_Document) doc;
    app->NewDocument("MDTV-XCAF", doc);
    STEPCAFControl_Reader reader;
    reader.SetColorMode(true);
    reader.SetNameMode(true);
    reader.SetLayerMode(false);
    if (reader.ReadFile(path) != IFSelect_RetDone) {
      app->Close(doc);
      return fail("could not read STEP file");
    }
    if (!reader.Transfer(doc)) {
      app->Close(doc);
      return fail("STEP transfer failed");
    }
    Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
    Handle(XCAFDoc_ColorTool) ct = XCAFDoc_DocumentTool::ColorTool(doc->Main());
    TDF_LabelSequence roots;
    st->GetFreeShapes(roots);
    std::vector<Part> parts;
    for (Standard_Integer i = 1; i <= roots.Length(); ++i) walk(st, ct, roots.Value(i), TopLoc_Location(), {}, false, Quantity_Color(), parts, 0);
    app->Close(doc);
    if (parts.empty()) return fail("STEP file has no shapes");
    auto arr = new SkShape*[parts.size()];
    std::string meta;
    for (size_t i = 0; i < parts.size(); ++i) {
      arr[i] = new SkShape{parts[i].shape};
      meta += parts[i].name;
      meta += '\t';
      if (parts[i].colored) {
        Standard_Real r, g, b;
        parts[i].color.Values(r, g, b, Quantity_TOC_sRGB);
        meta += std::to_string(r) + " " + std::to_string(g) + " " + std::to_string(b);
      } else {
        meta += "-";
      }
      meta += '\t';
      for (size_t k = 0; k < parts[i].path.size(); ++k) {
        if (k) meta += '\x1f';
        meta += parts[i].path[k];
      }
      meta += '\n';
    }
    g_meta = meta;
    *out = arr;
    *n = parts.size();
    return 0;
  });
}

// Writes shapes as named, colored parts (names as UTF-8; colors sRGB, a NaN red = none), each inside the nested
// assemblies its path names (0x1F between them; empty = at the top). schema as sk_write_step_schema.
int sk_write_step_doc(const SkShape* const* shapes, const char* const* names, const double* rgb, const char* const* paths, size_t n, const char* path, int schema) {
  return guard([&] {
    Handle(XCAFApp_Application) app = XCAFApp_Application::GetApplication();
    Handle(TDocStd_Document) doc;
    app->NewDocument("MDTV-XCAF", doc);
    Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
    Handle(XCAFDoc_ColorTool) ct = XCAFDoc_DocumentTool::ColorTool(doc->Main());
    std::vector<std::pair<std::string, TDF_Label>> assemblies;
    auto assembly = [&](const std::string& key, const std::string& name, const TDF_Label& parent) {
      for (auto& a : assemblies) if (a.first == key) return a.second;
      TDF_Label l = st->NewShape();
      TDataStd_Name::Set(l, TCollection_ExtendedString(name.c_str(), Standard_True));
      if (!parent.IsNull()) st->AddComponent(parent, l, TopLoc_Location());
      assemblies.push_back({key, l});
      return l;
    };
    for (size_t i = 0; i < n; ++i) {
      TDF_Label l = st->AddShape(shapes[i]->shape, false);
      if (names && names[i]) TDataStd_Name::Set(l, TCollection_ExtendedString(names[i], Standard_True));
      if (rgb && rgb[3 * i] == rgb[3 * i]) ct->SetColor(l, Quantity_Color(rgb[3 * i], rgb[3 * i + 1], rgb[3 * i + 2], Quantity_TOC_sRGB), XCAFDoc_ColorGen);
      std::string p = paths && paths[i] ? paths[i] : "";
      if (p.empty()) continue;
      TDF_Label parent;
      std::string key;
      size_t start = 0;
      while (start <= p.size()) {
        size_t end = p.find('\x1f', start);
        if (end == std::string::npos) end = p.size();
        std::string name = p.substr(start, end - start);
        key += '\x1f' + name;
        parent = assembly(key, name, parent);
        start = end + 1;
      }
      TDF_Label comp = st->AddComponent(parent, l, TopLoc_Location());
      if (names && names[i]) TDataStd_Name::Set(comp, TCollection_ExtendedString(names[i], Standard_True));
    }
    st->UpdateAssemblies();
    DESTEP_Parameters params;
    params.InitFromStatic();
    params.WriteSchema = schema == 1 ? DESTEP_Parameters::WriteMode_StepSchema_AP203
                       : schema == 3 ? DESTEP_Parameters::WriteMode_StepSchema_AP242DIS
                                     : DESTEP_Parameters::WriteMode_StepSchema_AP214IS;
    STEPCAFControl_Writer writer;
    writer.SetColorMode(true);
    writer.SetNameMode(true);
    bool ok = writer.Transfer(doc, params, STEPControl_AsIs);
    app->Close(doc);
    if (!ok) return fail("STEP transfer failed");
    if (writer.Write(path) != IFSelect_RetDone) return fail("could not write STEP file");
    return 0;
  });
}

int sk_write_iges(const SkShape* const* shapes, size_t n, const char* path) {
  return guard([&] {
    IGESControl_Writer writer("MM", 1);
    for (size_t i = 0; i < n; ++i) {
      if (!writer.AddShape(shapes[i]->shape)) return fail("IGES transfer failed");
    }
    writer.ComputeModel();
    if (!writer.Write(path)) return fail("could not write IGES file");
    return 0;
  });
}

// Reads an IGES file. Solids come out one by one; loose faces are sewn and made solid where they close.
int sk_read_iges(const char* path, SkShape*** out, size_t* n) {
  return guard([&] {
    IGESControl_Reader reader;
    if (reader.ReadFile(path) != IFSelect_RetDone) return fail("could not read IGES file");
    reader.TransferRoots();
    TopoDS_Shape root = reader.OneShape();
    if (root.IsNull()) return fail("IGES file has no shapes");
    std::vector<TopoDS_Shape> parts;
    for (TopExp_Explorer ex(root, TopAbs_SOLID); ex.More(); ex.Next()) parts.push_back(ex.Current());
    if (parts.empty()) {
      BRepBuilderAPI_Sewing sew(1e-3);
      sew.Add(root);
      sew.Perform();
      TopoDS_Shape sewn = sew.SewedShape();
      for (TopExp_Explorer ex(sewn, TopAbs_SHELL); ex.More(); ex.Next()) {
        TopoDS_Shell shell = TopoDS::Shell(ex.Current());
        if (BRep_Tool::IsClosed(shell)) {
          ShapeFix_Solid fix;
          fix.Init(BRepBuilderAPI_MakeSolid(shell).Solid());
          fix.Perform();
          parts.push_back(fix.Solid());
        } else {
          parts.push_back(shell);
        }
      }
      if (parts.empty()) parts.push_back(sewn);
    }
    auto arr = new SkShape*[parts.size()];
    for (size_t i = 0; i < parts.size(); ++i) arr[i] = new SkShape{parts[i]};
    *out = arr;
    *n = parts.size();
    return 0;
  });
}

// A shape from triangles (welded positions, 3 indices each): one planar face per triangle sharing edges, made a
// solid when the mesh is closed, else left a shell.
int sk_from_mesh(const double* xyz, size_t nverts, const uint32_t* idx, size_t ntris, SkShape** out) {
  return guard([&] {
    if (nverts < 3 || ntris < 1) return fail("empty mesh");
    Handle(Poly_Triangulation) tri = new Poly_Triangulation(int(nverts), int(ntris), false);
    for (size_t i = 0; i < nverts; ++i) tri->SetNode(int(i) + 1, gp_Pnt(xyz[3 * i], xyz[3 * i + 1], xyz[3 * i + 2]));
    for (size_t t = 0; t < ntris; ++t) {
      uint32_t a = idx[3 * t], b = idx[3 * t + 1], c = idx[3 * t + 2];
      if (a >= nverts || b >= nverts || c >= nverts) return fail("mesh index out of range");
      tri->SetTriangle(int(t) + 1, Poly_Triangle(int(a) + 1, int(b) + 1, int(c) + 1));
    }
    BRepBuilderAPI_MakeShapeOnMesh make(tri);
    make.Build();
    if (!make.IsDone()) return fail("could not build faces from the mesh");
    TopoDS_Shape shape = make.Shape();
    TopoDS_Shape result = shape;
    TopExp_Explorer ex(shape, TopAbs_SHELL);
    if (ex.More()) {
      TopoDS_Shell shell = TopoDS::Shell(ex.Current());
      if (BRep_Tool::IsClosed(shell)) {
        ShapeFix_Solid fix;
        fix.Init(BRepBuilderAPI_MakeSolid(shell).Solid());
        fix.Perform();
        result = fix.Solid();
      } else {
        result = shell;
      }
    }
    *out = new SkShape{result};
    return 0;
  });
}

void sk_shape_array_free(SkShape** arr) { delete[] arr; }

}  // extern "C"

namespace {

bool face_at(const TopoDS_Shape& s, uint32_t index, TopoDS_Face& out) {
  TopTools_IndexedMapOfShape faces;
  TopExp::MapShapes(s, TopAbs_FACE, faces);
  if (index >= uint32_t(faces.Extent())) return false;
  out = TopoDS::Face(faces(int(index) + 1));
  return true;
}

// Outward normal and centre of a planar face.
bool plane_of(const TopoDS_Face& f, gp_Pnt& origin, gp_Dir& normal) {
  BRepAdaptor_Surface surf(f);
  if (surf.GetType() != GeomAbs_Plane) {
    // A non-uniform scale leaves flat faces as B-spline surfaces; they still count as planes.
    Handle(Geom_Surface) g = BRep_Tool::Surface(f);
    if (g.IsNull() || !GeomLib_IsPlanarSurface(g, 1e-7).IsPlanar()) return false;
  }
  GProp_GProps props;
  BRepGProp::SurfaceProperties(f, props);
  origin = props.CentreOfMass();
  BRepGProp_Face gf(f);
  double u0, u1, v0, v1;
  gf.Bounds(u0, u1, v0, v1);
  gp_Pnt p;
  gp_Vec n;
  gf.Normal((u0 + u1) / 2, (v0 + v1) / 2, p, n);  // already flipped for face orientation
  if (n.Magnitude() <= 0.0) return false;
  normal = gp_Dir(n);
  return true;
}

}  // namespace

extern "C" {

// Planar face: writes centre + outward normal, returns 0. Non-planar: returns 1.
int sk_face_plane(const SkShape* s, uint32_t face, double* origin, double* normal) {
  return guard([&] {
    TopoDS_Face f;
    if (!face_at(s->shape, face, f)) return fail("no such face");
    gp_Pnt o;
    gp_Dir n;
    if (!plane_of(f, o, n)) return fail("face is not planar");
    origin[0] = o.X(); origin[1] = o.Y(); origin[2] = o.Z();
    normal[0] = n.X(); normal[1] = n.Y(); normal[2] = n.Z();
    return 0;
  });
}

// Pushes/pulls a planar face along its normal: positive adds material, negative removes it.
int sk_extrude_face(const SkShape* s, uint32_t face, double distance, SkShape** out) {
  return guard([&] {
    TopoDS_Face f;
    if (!face_at(s->shape, face, f)) return fail("no such face");
    gp_Pnt o;
    gp_Dir n;
    if (!plane_of(f, o, n)) return fail("only planar faces can be extruded yet");
    if (distance == 0.0) return fail("extrude distance is zero");
    BRepPrimAPI_MakePrism prism(f, gp_Vec(n) * distance);
    prism.Build();
    if (!prism.IsDone()) return fail("extrude failed");
    TopoDS_Shape r;
    if (distance > 0) {
      BRepAlgoAPI_Fuse op(s->shape, prism.Shape());
      if (!op.IsDone() || op.HasErrors()) return fail("extrude union failed");
      op.SimplifyResult();
      r = op.Shape();
    } else {
      BRepAlgoAPI_Cut op(s->shape, prism.Shape());
      if (!op.IsDone() || op.HasErrors()) return fail("extrude cut failed");
      op.SimplifyResult();
      r = op.Shape();
    }
    return put(out, r);
  });
}

int sk_fillet_edges(const SkShape* s, const uint32_t* ids, size_t n, double radius, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    BRepFilletAPI_MakeFillet mk(s->shape);
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      mk.Add(radius, TopoDS::Edge(edges(int(ids[i]) + 1)));
    }
    mk.Build();
    if (!mk.IsDone()) return fail("fillet failed (radius too large?)");
    return put_checked(out, mk.Shape(), s->shape, "fillet failed (radius too large?)", true);
  });
}

// A fillet whose radius runs from r1 at each edge's start to r2 at its end.
int sk_fillet_edges_var(const SkShape* s, const uint32_t* ids, size_t n, double r1, double r2, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    BRepFilletAPI_MakeFillet mk(s->shape);
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      mk.Add(r1, r2, TopoDS::Edge(edges(int(ids[i]) + 1)));
    }
    mk.Build();
    if (!mk.IsDone()) return fail("variable fillet failed (radius too large?)");
    return put_checked(out, mk.Shape(), s->shape, "variable fillet failed (radius too large?)", true);
  });
}

int sk_translate(const SkShape* s, double dx, double dy, double dz, SkShape** out) {
  return guard([&] {
    gp_Trsf t;
    t.SetTranslation(gp_Vec(dx, dy, dz));
    BRepBuilderAPI_Transform tr(s->shape, t, Standard_True);
    if (!tr.IsDone()) return fail("move failed");
    return put(out, tr.Shape());
  });
}

// The shape in OCCT's own text form (BRepTools), into sk_last_meta(); read back by sk_read_brep_text.
int sk_write_brep_text(const SkShape* s) {
  return guard([&] {
    std::ostringstream os;
    BRepTools::Write(s->shape, os);
    g_meta = os.str();
    return 0;
  });
}

int sk_read_brep_text(const char* text, SkShape** out) {
  return guard([&] {
    std::istringstream is{std::string(text)};
    TopoDS_Shape shape;
    BRep_Builder b;
    BRepTools::Read(shape, is, b);
    if (shape.IsNull()) return fail("no shape in the text");
    return put(out, shape);
  });
}

int sk_face_count(const SkShape* s, uint32_t* n) {
  return guard([&] {
    TopTools_IndexedMapOfShape faces;
    TopExp::MapShapes(s->shape, TopAbs_FACE, faces);
    *n = uint32_t(faces.Extent());
    return 0;
  });
}

}  // extern "C"

namespace {

bool polygon_face(const double* xyz, size_t n, TopoDS_Face& out) {
  if (n < 3) return false;
  BRepBuilderAPI_MakePolygon poly;
  for (size_t i = 0; i < n; ++i) poly.Add(gp_Pnt(xyz[3 * i], xyz[3 * i + 1], xyz[3 * i + 2]));
  poly.Close();
  if (!poly.IsDone()) return false;
  BRepBuilderAPI_MakeFace face(poly.Wire(), Standard_True);
  if (!face.IsDone()) return false;
  out = face.Face();
  return true;
}

int revolve_face(const TopoDS_Face& face, const double* axis, double angle, SkShape** out) {
  gp_Vec dir(axis[3], axis[4], axis[5]);
  if (dir.Magnitude() <= 0.0) return fail("revolve axis has no direction");
  gp_Ax1 ax(gp_Pnt(axis[0], axis[1], axis[2]), gp_Dir(dir));
  BRepPrimAPI_MakeRevol mk(face, ax, angle, Standard_True);
  mk.Build();
  if (!mk.IsDone()) return fail("revolve failed (does the profile cross the axis?)");
  return put(out, mk.Shape());
}

}  // namespace

extern "C" {

// Hollows a solid, opening the given faces; thickness grows inwards.
int sk_shell(const SkShape* s, const uint32_t* faces, size_t n, double thickness, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s->shape, TopAbs_FACE, map);
    TopTools_ListOfShape open;
    for (size_t i = 0; i < n; ++i) {
      if (faces[i] >= uint32_t(map.Extent())) return fail("no such face");
      open.Append(map(int(faces[i]) + 1));
    }
    if (n == 0) {
      // Nothing to open: a closed hollow, the outer skin plus the skin offset inwards as a void.
      TopExp_Explorer outer(s->shape, TopAbs_SHELL);
      if (!outer.More()) return fail("shell needs a solid");
      BRepOffsetAPI_MakeOffsetShape off;
      off.PerformByJoin(s->shape, -thickness, 1.0e-4);
      if (!off.IsDone()) return fail("shell failed (wall too thick?)");
      TopExp_Explorer inner(off.Shape(), TopAbs_SHELL);
      if (!inner.More()) return fail("shell failed (wall too thick?)");
      BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(outer.Current()));
      ms.Add(TopoDS::Shell(inner.Current().Reversed()));
      if (!ms.IsDone()) return fail("shell failed (wall too thick?)");
      TopoDS_Solid solid = ms.Solid();
      BRepLib::OrientClosedSolid(solid);
      return put_checked(out, solid, s->shape, "shell failed (wall too thick?)");
    }
    BRepOffsetAPI_MakeThickSolid mk;
    mk.MakeThickSolidByJoin(s->shape, open, -thickness, 1.0e-4);
    mk.Build();
    if (!mk.IsDone()) return fail("shell failed (wall too thick?)");
    return put_checked(out, mk.Shape(), s->shape, "shell failed (wall too thick?)");
  });
}

int sk_chamfer_edges(const SkShape* s, const uint32_t* ids, size_t n, double distance, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    BRepFilletAPI_MakeChamfer mk(s->shape);
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      mk.Add(distance, TopoDS::Edge(edges(int(ids[i]) + 1)));
    }
    mk.Build();
    if (!mk.IsDone()) return fail("chamfer failed (distance too large?)");
    return put_checked(out, mk.Shape(), s->shape, "chamfer failed (distance too large?)", true);
  });
}

// A chamfer of two distances: `d1` along `face` where the edge borders it (face < 0, or an edge that doesn't: its
// first face), `d2` along the other.
int sk_chamfer_edges2(const SkShape* s, const uint32_t* ids, size_t n, double d1, double d2, int32_t face, SkShape** out) {
  return guard([&] {
    TopoDS_Face on;
    if (face >= 0 && !face_at(s->shape, uint32_t(face), on)) return fail("no such face");
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    TopTools_IndexedDataMapOfShapeListOfShape edge_to_faces;
    TopExp::MapShapesAndAncestors(s->shape, TopAbs_EDGE, TopAbs_FACE, edge_to_faces);
    BRepFilletAPI_MakeChamfer mk(s->shape);
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      const TopoDS_Edge& e = TopoDS::Edge(edges(int(ids[i]) + 1));
      const TopTools_ListOfShape& faces = edge_to_faces.FindFromKey(e);
      if (faces.IsEmpty()) return fail("that edge has no face");
      TopoDS_Face first = TopoDS::Face(faces.First());
      for (TopTools_ListIteratorOfListOfShape it(faces); it.More(); it.Next())
        if (!on.IsNull() && it.Value().IsSame(on)) first = TopoDS::Face(it.Value());
      mk.Add(d1, d2, e, first);
    }
    mk.Build();
    if (!mk.IsDone()) return fail("chamfer failed (distance too large?)");
    return put_checked(out, mk.Shape(), s->shape, "chamfer failed (distance too large?)", true);
  });
}

// axis: origin xyz + direction xyz. angle in radians.
int sk_revolve_polygon(const double* xyz, size_t n, const double* axis, double angle, SkShape** out) {
  return guard([&] {
    TopoDS_Face face;
    if (!polygon_face(xyz, n, face)) return fail("profile is not a flat polygon");
    return revolve_face(face, axis, angle, out);
  });
}

int sk_revolve_circle(double cx, double cy, double cz, double nx, double ny, double nz, double r, const double* axis, double angle, SkShape** out) {
  return guard([&] {
    gp_Vec nv(nx, ny, nz);
    if (nv.Magnitude() <= 0.0 || r <= 0.0) return fail("bad circle");
    gp_Circ circ(gp_Ax2(gp_Pnt(cx, cy, cz), gp_Dir(nv)), r);
    TopoDS_Wire wire = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circ));
    BRepBuilderAPI_MakeFace face(wire, Standard_True);
    if (!face.IsDone()) return fail("circle face failed");
    return revolve_face(face.Face(), axis, angle, out);
  });
}

int sk_rotate(const SkShape* s, const double* axis, double angle, SkShape** out) {
  return guard([&] {
    gp_Vec dir(axis[3], axis[4], axis[5]);
    if (dir.Magnitude() <= 0.0) return fail("rotation axis has no direction");
    gp_Trsf t;
    t.SetRotation(gp_Ax1(gp_Pnt(axis[0], axis[1], axis[2]), gp_Dir(dir)), angle);
    BRepBuilderAPI_Transform tr(s->shape, t, Standard_True);
    if (!tr.IsDone()) return fail("rotate failed");
    return put(out, tr.Shape());
  });
}

int sk_mirror(const SkShape* s, double ox, double oy, double oz, double nx, double ny, double nz, SkShape** out) {
  return guard([&] {
    gp_Vec n(nx, ny, nz);
    if (n.Magnitude() <= 0.0) return fail("mirror plane has no normal");
    gp_Trsf t;
    t.SetMirror(gp_Ax2(gp_Pnt(ox, oy, oz), gp_Dir(n)));
    BRepBuilderAPI_Transform tr(s->shape, t, Standard_True);
    if (!tr.IsDone()) return fail("mirror failed");
    return put(out, tr.Shape());
  });
}

}  // extern "C"

namespace {

// One encoded planar curve → an edge. Advances `off` past its data; `ok` is false for degenerate curves.
// Returns false for an unknown kind. kinds: 0 line (a, b: 6 doubles), 1 circle (center, radius: 4),
// 2 three-point arc (a, mid, b: 9), 3 ellipse (center, major dir, ra, rb: 8), 4 spline (count, xyz…),
// 5 control-point spline (count, xyz…: a clamped B-spline of degree min(3, count-1) with uniform knots), 6 B-spline
// (pole count, degree, knot count, xyz…, every knot).
bool curve_edge(int kind, const double* data, size_t& off, const gp_Dir& nd, const gp_Dir& xd, TopoDS_Edge& e, bool& ok) {
  const double* d = data + off;
  ok = false;
  if (kind == 0) {
    gp_Pnt a(d[0], d[1], d[2]), b(d[3], d[4], d[5]);
    off += 6;
    if (a.Distance(b) > 1e-9) {
      BRepBuilderAPI_MakeEdge mk(a, b);
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else if (kind == 1) {
    gp_Pnt c(d[0], d[1], d[2]);
    double r = d[3];
    off += 4;
    if (r > 1e-9) {
      BRepBuilderAPI_MakeEdge mk(gp_Circ(gp_Ax2(c, nd, xd), r));
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else if (kind == 2) {
    gp_Pnt a(d[0], d[1], d[2]), m(d[3], d[4], d[5]), b(d[6], d[7], d[8]);
    off += 9;
    GC_MakeArcOfCircle arc(a, m, b);
    if (arc.IsDone()) {
      BRepBuilderAPI_MakeEdge mk(arc.Value());
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else if (kind == 3) {
    gp_Pnt c(d[0], d[1], d[2]);
    gp_Vec major(d[3], d[4], d[5]);
    double ra = d[6], rb = d[7];
    off += 8;
    if (ra >= rb && rb > 1e-9 && major.Magnitude() > 0.0) {
      BRepBuilderAPI_MakeEdge mk(gp_Elips(gp_Ax2(c, nd, gp_Dir(major)), ra, rb));
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else if (kind == 4) {
    size_t m = size_t(d[0]);
    off += 1 + 3 * m;
    if (m >= 2 && m <= 1000) {
      Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, int(m));
      for (size_t k = 0; k < m; ++k) pts->SetValue(int(k) + 1, gp_Pnt(d[1 + 3 * k], d[2 + 3 * k], d[3 + 3 * k]));
      GeomAPI_Interpolate interp(pts, Standard_False, 1e-7);
      interp.Perform();
      if (interp.IsDone()) {
        BRepBuilderAPI_MakeEdge mk(interp.Curve());
        ok = mk.IsDone();
        if (ok) e = mk.Edge();
      }
    }
  } else if (kind == 6) {
    // A B-spline given whole: pole count, degree, knot count, poles, every knot (ends repeated).
    size_t m = size_t(d[0]), deg = size_t(d[1]), nk = size_t(d[2]);
    off += 3 + 3 * m + nk;
    if (m >= 2 && m <= 1000 && deg >= 1 && deg <= 3 && deg < m && nk == m + deg + 1) {
      TColgp_Array1OfPnt poles(1, int(m));
      for (size_t k = 0; k < m; ++k) poles.SetValue(int(k) + 1, gp_Pnt(d[3 + 3 * k], d[4 + 3 * k], d[5 + 3 * k]));
      const double* ks = d + 3 + 3 * m;
      std::vector<double> uk;
      std::vector<int> um;
      for (size_t k = 0; k < nk; ++k) {
        if (!uk.empty() && ks[k] == uk.back()) um.back() += 1;
        else if (uk.empty() || ks[k] > uk.back()) { uk.push_back(ks[k]); um.push_back(1); }
        else return true;
      }
      TColStd_Array1OfReal knots(1, int(uk.size()));
      TColStd_Array1OfInteger mults(1, int(uk.size()));
      for (size_t k = 0; k < uk.size(); ++k) {
        knots.SetValue(int(k) + 1, uk[k]);
        mults.SetValue(int(k) + 1, um[k]);
      }
      Handle(Geom_BSplineCurve) curve = new Geom_BSplineCurve(poles, knots, mults, int(deg));
      BRepBuilderAPI_MakeEdge mk(curve);
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else if (kind == 5) {
    size_t m = size_t(d[0]);
    off += 1 + 3 * m;
    if (m >= 2 && m <= 1000) {
      int deg = int(m) - 1 < 3 ? int(m) - 1 : 3;
      int nk = int(m) - deg + 1;
      TColgp_Array1OfPnt poles(1, int(m));
      for (size_t k = 0; k < m; ++k) poles.SetValue(int(k) + 1, gp_Pnt(d[1 + 3 * k], d[2 + 3 * k], d[3 + 3 * k]));
      TColStd_Array1OfReal knots(1, nk);
      TColStd_Array1OfInteger mults(1, nk);
      for (int k = 1; k <= nk; ++k) {
        knots.SetValue(k, double(k - 1));
        mults.SetValue(k, (k == 1 || k == nk) ? deg + 1 : 1);
      }
      Handle(Geom_BSplineCurve) curve = new Geom_BSplineCurve(poles, knots, mults, deg);
      BRepBuilderAPI_MakeEdge mk(curve);
      ok = mk.IsDone();
      if (ok) e = mk.Edge();
    }
  } else {
    return false;
  }
  return true;
}

bool plane_dirs(const double* plane, gp_Dir& nd, gp_Dir& xd) {
  gp_Vec xv(plane[3], plane[4], plane[5]);
  gp_Vec yv(plane[6], plane[7], plane[8]);
  gp_Vec nv = xv.Crossed(yv);
  if (xv.Magnitude() <= 0.0 || nv.Magnitude() <= 0.0) return false;
  xd = gp_Dir(xv);
  nd = gp_Dir(nv);
  return true;
}

}  // namespace

extern "C" {

// The closed areas a set of planar curves encloses, as faces (one per cell, holes included), the way a
// sketch splits into regions. plane: origin, x axis, y axis (9 doubles). Curves are encoded as in curve_edge.
int sk_planar_regions(const double* plane, const int* kinds, const double* data, size_t n, SkShape*** out, size_t* count) {
  return guard([&] {
    *out = nullptr;
    *count = 0;
    gp_Pnt origin(plane[0], plane[1], plane[2]);
    gp_Vec xv(plane[3], plane[4], plane[5]);
    gp_Vec yv(plane[6], plane[7], plane[8]);
    gp_Vec nv = xv.Crossed(yv);
    if (xv.Magnitude() <= 0.0 || nv.Magnitude() <= 0.0) return fail("bad sketch plane");
    gp_Dir xd(xv), nd(nv);
    gp_Ax3 ax(origin, nd, xd);
    gp_Pln pln(ax);

    TopTools_ListOfShape tools;
    Bnd_Box box;
    size_t off = 0;
    for (size_t i = 0; i < n; ++i) {
      TopoDS_Edge e;
      bool ok = false;
      if (!curve_edge(kinds[i], data, off, nd, xd, e, ok)) return fail("unknown curve kind");
      if (ok) {
        tools.Append(e);
        BRepBndLib::Add(e, box);
      }
    }
    if (tools.IsEmpty() || box.IsVoid()) return 0;

    // A face bigger than everything, split by the curves; cells touching its border are "outside".
    double xmin, ymin, zmin, xmax, ymax, zmax;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    double umin = 1e300, umax = -1e300, vmin = 1e300, vmax = -1e300;
    for (int i = 0; i < 8; ++i) {
      gp_Pnt c(i & 1 ? xmax : xmin, i & 2 ? ymax : ymin, i & 4 ? zmax : zmin);
      double u, v;
      ElSLib::Parameters(pln, c, u, v);
      umin = std::min(umin, u); umax = std::max(umax, u);
      vmin = std::min(vmin, v); vmax = std::max(vmax, v);
    }
    double pad = std::max(umax - umin, vmax - vmin) * 0.25 + 1.0;
    umin -= pad; umax += pad; vmin -= pad; vmax += pad;
    BRepBuilderAPI_MakeFace big(pln, umin, umax, vmin, vmax);
    if (!big.IsDone()) return fail("could not build the sketch plane");

    BRepAlgoAPI_Splitter split;
    TopTools_ListOfShape args;
    args.Append(big.Face());
    split.SetArguments(args);
    split.SetTools(tools);
    split.Build();
    if (!split.IsDone() || split.HasErrors()) return fail("could not split the sketch into regions");

    std::vector<TopoDS_Shape> cells;
    const double eps = pad * 1e-3;
    for (TopExp_Explorer ex(split.Shape(), TopAbs_FACE); ex.More(); ex.Next()) {
      const TopoDS_Face& f = TopoDS::Face(ex.Current());
      double u0, u1, v0, v1;
      BRepTools::UVBounds(f, u0, u1, v0, v1);
      bool outer = u0 <= umin + eps || u1 >= umax - eps || v0 <= vmin + eps || v1 >= vmax - eps;
      if (!outer) cells.push_back(f);
    }
    if (cells.empty()) return 0;
    auto arr = new SkShape*[cells.size()];
    for (size_t i = 0; i < cells.size(); ++i) arr[i] = new SkShape{cells[i]};
    *out = arr;
    *count = cells.size();
    return 0;
  });
}

// Extrudes any planar face along a vector.
int sk_prism(const SkShape* face, double dx, double dy, double dz, SkShape** out) {
  return guard([&] {
    gp_Vec v(dx, dy, dz);
    if (v.Magnitude() <= 0.0) return fail("extrude distance is zero");
    BRepPrimAPI_MakePrism mk(face->shape, v);
    mk.Build();
    if (!mk.IsDone()) return fail("extrude failed");
    return put(out, mk.Shape());
  });
}

int sk_revolve_shape(const SkShape* face, const double* axis, double angle, SkShape** out) {
  return guard([&] {
    gp_Vec dir(axis[3], axis[4], axis[5]);
    if (dir.Magnitude() <= 0.0) return fail("revolve axis has no direction");
    BRepPrimAPI_MakeRevol mk(face->shape, gp_Ax1(gp_Pnt(axis[0], axis[1], axis[2]), gp_Dir(dir)), angle, Standard_True);
    mk.Build();
    if (!mk.IsDone()) return fail("revolve failed (does the profile cross the axis?)");
    return put(out, mk.Shape());
  });
}

}  // extern "C"

namespace {

// kind: 0 the whole shape, 1 a face, 2 an edge (index in TopExp map order).
bool sub_shape(const TopoDS_Shape& s, int kind, uint32_t index, TopoDS_Shape& out) {
  if (kind == 0) {
    out = s;
    return true;
  }
  TopTools_IndexedMapOfShape map;
  TopExp::MapShapes(s, kind == 1 ? TopAbs_FACE : TopAbs_EDGE, map);
  if (index >= uint32_t(map.Extent())) return false;
  out = map(int(index) + 1);
  return true;
}

}  // namespace

extern "C" {

// Size of a face (area) or an edge (length), and its center.
int sk_sub_measure(const SkShape* s, int kind, uint32_t index, double* value, double* center) {
  return guard([&] {
    TopoDS_Shape sub;
    if (!sub_shape(s->shape, kind, index, sub)) return fail("no such face or edge");
    GProp_GProps p;
    if (kind == 1) {
      BRepGProp::SurfaceProperties(sub, p);
    } else if (kind == 2) {
      BRepGProp::LinearProperties(sub, p);
    } else {
      BRepGProp::VolumeProperties(sub, p);
    }
    *value = p.Mass();
    gp_Pnt c = p.CentreOfMass();
    center[0] = c.X(); center[1] = c.Y(); center[2] = c.Z();
    return 0;
  });
}

// Shortest distance between two (sub-)shapes, with the closest points.
int sk_distance(const SkShape* a, int ka, uint32_t ia, const SkShape* b, int kb, uint32_t ib, double* d, double* pa, double* pb) {
  return guard([&] {
    TopoDS_Shape sa, sb;
    if (!sub_shape(a->shape, ka, ia, sa) || !sub_shape(b->shape, kb, ib, sb)) return fail("no such face or edge");
    BRepExtrema_DistShapeShape dist(sa, sb);
    if (!dist.IsDone() || dist.NbSolution() < 1) return fail("could not measure the distance");
    *d = dist.Value();
    gp_Pnt p1 = dist.PointOnShape1(1), p2 = dist.PointOnShape2(1);
    pa[0] = p1.X(); pa[1] = p1.Y(); pa[2] = p1.Z();
    pb[0] = p2.X(); pb[1] = p2.Y(); pb[2] = p2.Z();
    return 0;
  });
}

int sk_scale(const SkShape* s, double cx, double cy, double cz, double sx, double sy, double sz, SkShape** out) {
  return guard([&] {
    if (!(sx > 0.0) || !(sy > 0.0) || !(sz > 0.0)) return fail("scale factors must be positive");
    gp_Pnt c(cx, cy, cz);
    if (std::abs(sx - sy) < 1e-12 && std::abs(sx - sz) < 1e-12) {
      gp_Trsf t;
      t.SetScale(c, sx);
      BRepBuilderAPI_Transform tr(s->shape, t, Standard_True);
      if (!tr.IsDone()) return fail("scale failed");
      return put(out, tr.Shape());
    }
    // Non-uniform: x' = c + S (x - c), so the translation part is c - S c.
    gp_GTrsf g;
    g.SetValue(1, 1, sx);
    g.SetValue(2, 2, sy);
    g.SetValue(3, 3, sz);
    g.SetTranslationPart(gp_XYZ(cx * (1 - sx), cy * (1 - sy), cz * (1 - sz)));
    BRepBuilderAPI_GTransform tr(s->shape, g, Standard_True);
    if (!tr.IsDone()) return fail("scale failed");
    // The copy carries the old faces' display triangles, which no longer fit; drop them so it's meshed anew.
    TopoDS_Shape r = tr.Shape();
    BRepTools::Clean(r);
    return put(out, r);
  });
}

// Moves the given faces along their normals by `distance` (curved faces too); the neighbours stretch to follow.
namespace {

// Offsets the given faces of `shape` (others stay); an empty shape when OCCT can't.
TopoDS_Shape offset_some(const TopoDS_Shape& shape, const std::vector<TopoDS_Face>& faces, double distance) {
  BRepOffset_MakeOffset mo;
  mo.Initialize(shape, 0.0, 1.0e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Intersection, Standard_False);
  for (const auto& f : faces) mo.SetOffsetOnFace(f, distance);
  mo.MakeOffsetShape();
  if (!mo.IsDone()) return TopoDS_Shape();
  TopoDS_Shape r = mo.Shape();
  if (r.IsNull()) return r;
  // The result can come back as a shell; close it into a solid.
  if (r.ShapeType() != TopAbs_SOLID && r.ShapeType() != TopAbs_COMPSOLID && r.ShapeType() != TopAbs_COMPOUND) {
    if (r.ShapeType() != TopAbs_SHELL) return TopoDS_Shape();
    BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(r));
    if (!ms.IsDone()) return TopoDS_Shape();
    r = ms.Solid();
  }
  BRepCheck_Analyzer check(r);
  if (!check.IsValid() || !(volume_of(r) > 1e-9)) return TopoDS_Shape();
  return r;
}

// Outward normal of a face at a point on (or near) it.
bool normal_at(const TopoDS_Face& f, const gp_Pnt& p, gp_Dir& n) {
  Handle(Geom_Surface) g = BRep_Tool::Surface(f);
  if (g.IsNull()) return false;
  ShapeAnalysis_Surface sas(g);
  gp_Pnt2d uv = sas.ValueOfUV(p, 1e-6);
  BRepGProp_Face gf(f);
  gp_Pnt q;
  gp_Vec v;
  gf.Normal(uv.X(), uv.Y(), q, v);  // flipped for the face's orientation
  if (v.Magnitude() <= 1e-12) return false;
  n = gp_Dir(v);
  return true;
}

// Whether two faces meet smoothly (tangent) along an edge, judged at its middle.
bool tangent_along(const TopoDS_Edge& e, const TopoDS_Face& a, const TopoDS_Face& b) {
  BRepAdaptor_Curve c(e);
  gp_Pnt m = c.Value((c.FirstParameter() + c.LastParameter()) / 2.0);
  gp_Dir na, nb;
  return normal_at(a, m, na) && normal_at(b, m, nb) && na.Angle(nb) < 1e-3;
}

// Whether two faces lie on the same plane, cylinder, cone or sphere.
bool same_surface(const TopoDS_Face& a, const TopoDS_Face& b) {
  BRepAdaptor_Surface sa(a), sb(b);
  if (sa.GetType() != sb.GetType()) return false;
  const double tol = 1e-6;
  switch (sa.GetType()) {
    case GeomAbs_Plane:
      return sa.Plane().Axis().Direction().IsParallel(sb.Plane().Axis().Direction(), 1e-9) && sa.Plane().Distance(sb.Plane().Location()) < tol;
    case GeomAbs_Cylinder:
      return std::abs(sa.Cylinder().Radius() - sb.Cylinder().Radius()) < tol && sa.Cylinder().Axis().IsCoaxial(sb.Cylinder().Axis(), 1e-9, tol);
    case GeomAbs_Cone:
      return std::abs(sa.Cone().SemiAngle() - sb.Cone().SemiAngle()) < 1e-9 && sa.Cone().Apex().Distance(sb.Cone().Apex()) < tol &&
             sa.Cone().Axis().Direction().IsParallel(sb.Cone().Axis().Direction(), 1e-9);
    case GeomAbs_Sphere:
      return std::abs(sa.Sphere().Radius() - sb.Sphere().Radius()) < tol && sa.Sphere().Location().Distance(sb.Sphere().Location()) < tol;
    default:
      return false;
  }
}

// A round blend's radius: a cylinder's, or a torus' tube.
double blend_radius(const TopoDS_Face& f) {
  BRepAdaptor_Surface sf(f);
  if (sf.GetType() == GeomAbs_Torus) return sf.Torus().MinorRadius();
  if (sf.GetType() == GeomAbs_Cylinder) return sf.Cylinder().Radius();
  return -1.0;
}

// Moving flat faces that have fillets running along them, the way a direct modeler does: the fillets come off, the
// faces move, and the same fillets go back on between the moved faces and their old neighbours.
TopoDS_Shape offset_keeping_fillets(const TopoDS_Shape& shape, const std::vector<TopoDS_Face>& faces, double distance) {
  TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  auto other = [&](const TopoDS_Edge& e, const TopoDS_Face& f) -> TopoDS_Face {
    if (!edge_faces.Contains(e)) return TopoDS_Face();
    for (TopTools_ListIteratorOfListOfShape it(edge_faces.FindFromKey(e)); it.More(); it.Next())
      if (!it.Value().IsSame(f)) return TopoDS::Face(it.Value());
    return TopoDS_Face();
  };
  struct Blend { TopoDS_Face face; TopoDS_Face moved; TopoDS_Face partner; double radius; };
  std::vector<Blend> blends;
  TopTools_ListOfShape remove;
  for (const auto& f : faces) {
    if (BRepAdaptor_Surface(f).GetType() != GeomAbs_Plane) return TopoDS_Shape();
    for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
      TopoDS_Edge e = TopoDS::Edge(ex.Current());
      TopoDS_Face b = other(e, f);
      double r = b.IsNull() ? -1.0 : blend_radius(b);
      if (r <= 0.0 || !tangent_along(e, f, b) || remove.Contains(b)) continue;
      // The blend's partner: the face it meets smoothly on its far side.
      TopoDS_Face partner;
      for (TopExp_Explorer eb(b, TopAbs_EDGE); eb.More() && partner.IsNull(); eb.Next()) {
        TopoDS_Edge e2 = TopoDS::Edge(eb.Current());
        TopoDS_Face g = other(e2, b);
        if (!g.IsNull() && !g.IsSame(f) && tangent_along(e2, b, g)) partner = g;
      }
      if (partner.IsNull()) return TopoDS_Shape();
      remove.Append(b);
      blends.push_back({b, f, partner, r});
    }
  }
  if (blends.empty()) return TopoDS_Shape();
  BRepAlgoAPI_Defeaturing df;
  df.SetShape(shape);
  df.AddFacesToRemove(remove);
  df.SetRunParallel(Standard_False);
  df.SetToFillHistory(Standard_True);
  df.Build();
  if (!df.IsDone() || df.HasErrors()) return TopoDS_Shape();
  TopoDS_Shape bare = df.Shape();
  // The moved faces in the defeatured shape (they grew to fill where the fillets were).
  std::vector<TopoDS_Face> targets;
  TopTools_IndexedMapOfShape bare_faces;
  TopExp::MapShapes(bare, TopAbs_FACE, bare_faces);
  for (const auto& f : faces) {
    for (int i = 1; i <= bare_faces.Extent(); ++i) {
      TopoDS_Face g = TopoDS::Face(bare_faces(i));
      if (same_surface(f, g)) targets.push_back(g);
    }
  }
  if (targets.empty()) return TopoDS_Shape();
  TopoDS_Shape moved = offset_some(bare, targets, distance);
  if (moved.IsNull()) return moved;
  // Join faces and edges the round trip cut into pieces, so each fillet goes back on as one face.
  {
    ShapeUpgrade_UnifySameDomain unify(moved, Standard_True, Standard_True, Standard_False);
    unify.Build();
    moved = unify.Shape();
  }
  // Fillet again where a moved face meets its blend's partner.
  TopTools_IndexedDataMapOfShapeListOfShape moved_edges;
  TopExp::MapShapesAndAncestors(moved, TopAbs_EDGE, TopAbs_FACE, moved_edges);
  BRepFilletAPI_MakeFillet mk(moved);
  int added = 0;
  for (const auto& bl : blends) {
    gp_Pln was = BRepAdaptor_Surface(bl.moved).Plane();
    gp_Dir n;
    BRepGProp_Face gf(bl.moved);
    double u0, u1, v0, v1;
    gf.Bounds(u0, u1, v0, v1);
    gp_Pnt p;
    gp_Vec nv;
    gf.Normal((u0 + u1) / 2, (v0 + v1) / 2, p, nv);
    if (nv.Magnitude() <= 0.0) return TopoDS_Shape();
    gp_Pln now = was.Translated(gp_Vec(gp_Dir(nv)) * distance);
    for (int i = 1; i <= moved_edges.Extent(); ++i) {
      const TopTools_ListOfShape& fl = moved_edges(i);
      if (fl.Extent() != 2) continue;
      TopoDS_Face a = TopoDS::Face(fl.First()), b = TopoDS::Face(fl.Last());
      auto on_now = [&](const TopoDS_Face& g) {
        BRepAdaptor_Surface sg(g);
        return sg.GetType() == GeomAbs_Plane && sg.Plane().Axis().Direction().IsParallel(now.Axis().Direction(), 1e-9) &&
               now.Distance(sg.Plane().Location()) < 1e-6;
      };
      if ((on_now(a) && same_surface(b, bl.partner)) || (on_now(b) && same_surface(a, bl.partner))) {
        mk.Add(bl.radius, TopoDS::Edge(moved_edges.FindKey(i)));
        ++added;
      }
    }
  }
  if (added == 0) return TopoDS_Shape();
  mk.Build();
  if (!mk.IsDone()) return TopoDS_Shape();
  // Join faces and edges the round trip cut into pieces.
  ShapeUpgrade_UnifySameDomain unify(mk.Shape(), Standard_True, Standard_True, Standard_False);
  unify.Build();
  TopoDS_Shape r = unify.Shape();
  BRepCheck_Analyzer check(r);
  if (!check.IsValid() || !(volume_of(r) > 1e-9)) return TopoDS_Shape();
  return r;
}

// Pulling flat faces out past where they'd shrink to nothing (a chamfer pulled into its corner) leaves them gone,
// the neighbours meeting again (observed 367–368): the faces come off, if what's left lies behind every moved plane.
TopoDS_Shape offset_past_collapse(const TopoDS_Shape& shape, const std::vector<TopoDS_Face>& faces, double distance) {
  if (distance <= 0.0) return TopoDS_Shape();
  std::vector<gp_Pln> moved;
  TopTools_ListOfShape remove;
  for (const auto& f : faces) {
    BRepAdaptor_Surface sf(f);
    if (sf.GetType() != GeomAbs_Plane) return TopoDS_Shape();
    BRepGProp_Face gf(f);
    double u0, u1, v0, v1;
    gf.Bounds(u0, u1, v0, v1);
    gp_Pnt p;
    gp_Vec n;
    gf.Normal((u0 + u1) / 2, (v0 + v1) / 2, p, n);
    if (n.Magnitude() <= 0.0) return TopoDS_Shape();
    moved.emplace_back(p.Translated(gp_Vec(gp_Dir(n)) * distance), gp_Dir(n));
    remove.Append(f);
  }
  BRepAlgoAPI_Defeaturing df;
  df.SetShape(shape);
  df.AddFacesToRemove(remove);
  df.SetRunParallel(Standard_False);
  df.Build();
  if (!df.IsDone() || df.HasErrors()) return TopoDS_Shape();
  TopoDS_Shape r = df.Shape();
  for (TopExp_Explorer ex(r, TopAbs_VERTEX); ex.More(); ex.Next()) {
    gp_Pnt q = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
    for (const auto& pl : moved) {
      if (gp_Vec(pl.Location(), q).Dot(gp_Vec(pl.Axis().Direction())) > 1e-6) return TopoDS_Shape();
    }
  }
  BRepCheck_Analyzer check(r);
  if (!check.IsValid() || !(volume_of(r) > 1e-9)) return TopoDS_Shape();
  return r;
}

}  // namespace

// A flat face's outward normal and its plane's offset along it (n·p for any point p on the plane).
bool face_level(const TopoDS_Face& f, gp_Dir& n, double& h) {
  BRepAdaptor_Surface sf(f);
  if (sf.GetType() != GeomAbs_Plane) return false;
  gp_Pln pl = sf.Plane();
  n = pl.Axis().Direction();
  if (f.Orientation() == TopAbs_REVERSED) n.Reverse();
  h = gp_Vec(pl.Location().XYZ()).Dot(gp_Vec(n));
  return true;
}

// Whether an offset's result is believable: pushing faces out only adds material, never reaches back behind the body
// along a pushed flat face's normal nor past the face's new place; pulling them in only takes material away. (OCCT can come back with a valid
// but runaway solid, e.g. where two drafted halves meet at a ridge.)
bool plausible_offset(const TopoDS_Shape& before, const std::vector<TopoDS_Face>& faces, double distance, const TopoDS_Shape& after) {
  double v0 = volume_of(before), v1 = volume_of(after);
  if (distance > 0.0 ? !(v1 > v0) : !(v1 < v0)) return false;
  Bnd_Box b0, b1;
  BRepBndLib::AddOptimal(before, b0, Standard_False, Standard_False);
  BRepBndLib::AddOptimal(after, b1, Standard_False, Standard_False);
  if (b0.IsVoid() || b1.IsVoid()) return false;
  double tol = 1e-6 * std::sqrt(b0.SquareExtent()) + 1e-7;
  double a[6], b[6];
  b0.Get(a[0], a[1], a[2], a[3], a[4], a[5]);
  b1.Get(b[0], b[1], b[2], b[3], b[4], b[5]);
  if (distance < 0.0) return b[0] >= a[0] - tol && b[1] >= a[1] - tol && b[2] >= a[2] - tol && b[3] <= a[3] + tol && b[4] <= a[4] + tol && b[5] <= a[5] + tol;
  auto lowest = [](const double* c, const gp_Dir& n) {
    double m = 1e300;
    for (int i = 0; i < 8; ++i) m = std::min(m, (i & 1 ? c[3] : c[0]) * n.X() + (i & 2 ? c[4] : c[1]) * n.Y() + (i & 4 ? c[5] : c[2]) * n.Z());
    return m;
  };
  for (const auto& f : faces) {
    gp_Dir n;
    double h;
    if (!face_level(f, n, h)) continue;
    gp_Dir back = n.Reversed();
    // Nothing behind the body, and nothing past the face's new place unless the body already reached further.
    if (lowest(b, n) < lowest(a, n) - tol || -lowest(b, back) > std::max(-lowest(a, back), h + distance) + tol) return false;
  }
  return true;
}

// Delete Face: the faces come out and their neighbours extend to close the body (BRepAlgoAPI_Defeaturing).
int sk_remove_faces(const SkShape* s, const uint32_t* ids, size_t n, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s->shape, TopAbs_FACE, map);
    BRepAlgoAPI_Defeaturing mk;
    mk.SetShape(s->shape);
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(map.Extent())) return fail("no such face");
      mk.AddFaceToRemove(map(int(ids[i]) + 1));
    }
    mk.SetRunParallel(Standard_True);
    mk.Build();
    if (!mk.IsDone() || mk.HasErrors()) return fail("the hole the face leaves can't be filled");
    // When it can't close the gap it hands the body back untouched (with a warning): that's a refusal too.
    TopTools_IndexedMapOfShape after;
    TopExp::MapShapes(mk.Shape(), TopAbs_FACE, after);
    if (mk.HasWarnings() || after.Extent() >= map.Extent()) return fail("the hole the face leaves can't be filled");
    BRepCheck_Analyzer check(mk.Shape());
    if (!check.IsValid()) return fail("the hole the face leaves can't be filled");
    return put(out, mk.Shape());
  });
}

int sk_offset_faces(const SkShape* s, const uint32_t* ids, size_t n, double distance, SkShape** out) {
  return guard([&] {
    if (n == 0) return fail("no faces to offset");
    if (distance == 0.0) return fail("offset distance is zero");
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s->shape, TopAbs_FACE, map);
    std::vector<TopoDS_Face> faces;
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(map.Extent())) return fail("no such face");
      faces.push_back(TopoDS::Face(map(int(ids[i]) + 1)));
    }
    // A face with a fillet running along it moves with the fillet riding along at the same radius, as the reference
    // does (observed 371). Tried first: a plain offset would "succeed" by growing the tangent fillet instead.
    auto believable = [&](TopoDS_Shape r) { return !r.IsNull() && plausible_offset(s->shape, faces, distance, r) ? r : TopoDS_Shape(); };
    TopoDS_Shape r = believable(offset_keeping_fillets(s->shape, faces, distance));
    if (r.IsNull()) r = believable(offset_some(s->shape, faces, distance));
    if (r.IsNull()) r = believable(offset_past_collapse(s->shape, faces, distance));
    if (r.IsNull()) return fail("offset failed for this distance");
    return put(out, r);
  });
}

// The solids of a shape, each on its own.
int sk_solids(const SkShape* s, SkShape*** out, size_t* count) {
  return guard([&] {
    *out = nullptr;
    *count = 0;
    std::vector<TopoDS_Shape> parts;
    for (TopExp_Explorer ex(s->shape, TopAbs_SOLID); ex.More(); ex.Next()) parts.push_back(ex.Current());
    if (parts.empty()) return 0;
    auto arr = new SkShape*[parts.size()];
    for (size_t i = 0; i < parts.size(); ++i) arr[i] = new SkShape{parts[i]};
    *out = arr;
    *count = parts.size();
    return 0;
  });
}

// Cuts a solid with the plane through (ox, oy, oz) with normal (nx, ny, nz); returns the pieces.
int sk_split_plane(const SkShape* s, double ox, double oy, double oz, double nx, double ny, double nz, SkShape*** out, size_t* count) {
  return guard([&] {
    *out = nullptr;
    *count = 0;
    gp_Vec nv(nx, ny, nz);
    if (nv.Magnitude() <= 0.0) return fail("split plane has no normal");
    Bnd_Box box;
    BRepBndLib::Add(s->shape, box);
    if (box.IsVoid()) return fail("nothing to split");
    double size = std::sqrt(box.SquareExtent()) * 2.0 + 1.0;
    gp_Pln pln(gp_Pnt(ox, oy, oz), gp_Dir(nv));
    BRepBuilderAPI_MakeFace mf(pln, -size, size, -size, size);
    if (!mf.IsDone()) return fail("split plane failed");
    BRepAlgoAPI_Splitter sp;
    TopTools_ListOfShape args, tools;
    args.Append(s->shape);
    tools.Append(mf.Face());
    sp.SetArguments(args);
    sp.SetTools(tools);
    sp.Build();
    if (!sp.IsDone() || sp.HasErrors()) return fail("split failed");
    std::vector<TopoDS_Shape> parts;
    for (TopExp_Explorer ex(sp.Shape(), TopAbs_SOLID); ex.More(); ex.Next()) parts.push_back(ex.Current());
    if (parts.size() < 2) return fail("the plane doesn't cut this body");
    auto arr = new SkShape*[parts.size()];
    for (size_t i = 0; i < parts.size(); ++i) arr[i] = new SkShape{parts[i]};
    *out = arr;
    *count = parts.size();
    return 0;
  });
}

}  // extern "C"

extern "C" {

// A copy of one face of a shape, as its own shape (for lofting or sweeping a body's face).
int sk_face_shape(const SkShape* s, uint32_t face, SkShape** out) {
  return guard([&] {
    TopoDS_Face f;
    if (!face_at(s->shape, face, f)) return fail("no such face");
    return put(out, f);
  });
}

// Project as Edge: the tools' edges projected onto one face along its normal (only what lands on it) and
// imprinted there, splitting the face; the body keeps its volume.
// `edge[i]` < 0 takes every edge of tools[i], else only that edge.
int sk_imprint(const SkShape* s, uint32_t face, const SkShape* const* tools, const int64_t* edge, size_t n, SkShape** out) {
  return guard([&] {
    TopoDS_Face f;
    if (!face_at(s->shape, face, f)) return fail("no such face");
    TopTools_IndexedMapOfShape edges;
    for (size_t i = 0; i < n; ++i) {
      if (edge[i] < 0) {
        TopExp::MapShapes(tools[i]->shape, TopAbs_EDGE, edges);
        continue;
      }
      TopTools_IndexedMapOfShape own;
      TopExp::MapShapes(tools[i]->shape, TopAbs_EDGE, own);
      if (edge[i] >= own.Extent()) return fail("no such edge");
      edges.Add(own(int(edge[i]) + 1));
    }
    if (edges.Extent() == 0) return fail("nothing to project");
    BRepOffsetAPI_NormalProjection proj(f);
    for (int i = 1; i <= edges.Extent(); ++i) proj.Add(edges(i));
    proj.Build();
    if (!proj.IsDone()) return fail("projection failed");
    TopTools_ListOfShape args, cut;
    for (TopExp_Explorer ex(proj.Projection(), TopAbs_EDGE); ex.More(); ex.Next()) cut.Append(ex.Current());
    if (cut.IsEmpty()) return fail("nothing lands on that face");
    args.Append(s->shape);
    BRepAlgoAPI_Splitter sp;
    sp.SetArguments(args);
    sp.SetTools(cut);
    sp.Build();
    if (!sp.IsDone() || sp.HasErrors()) return fail("imprint failed");
    // One solid in, one solid out (the splitter wraps it in a compound).
    TopExp_Explorer solid(sp.Shape(), TopAbs_SOLID);
    if (!solid.More()) return fail("imprint lost the body");
    TopoDS_Shape result = solid.Current();
    solid.Next();
    if (solid.More()) result = sp.Shape();
    return put(out, result);
  });
}

// A solid through the outer boundaries of planar faces, in order (holes are ignored).
int sk_loft(const SkShape* const* faces, size_t n, SkShape** out);

// A loft with straight (ruled) walls between the profiles instead of smooth ones.
int sk_loft_ruled(const SkShape* const* faces, size_t n, int ruled, SkShape** out);

int sk_loft(const SkShape* const* faces, size_t n, SkShape** out) { return sk_loft_ruled(faces, n, 0, out); }

int sk_loft_ruled(const SkShape* const* faces, size_t n, int ruled, SkShape** out) {
  return guard([&] {
    if (n < 2) return fail("loft needs at least two profiles");
    BRepOffsetAPI_ThruSections mk(Standard_True, ruled ? Standard_True : Standard_False, 1e-6);
    for (size_t i = 0; i < n; ++i) {
      TopExp_Explorer ex(faces[i]->shape, TopAbs_FACE);
      if (!ex.More()) return fail("a loft profile isn't a face");
      TopoDS_Wire w = BRepTools::OuterWire(TopoDS::Face(ex.Current()));
      if (w.IsNull()) return fail("a loft profile has no boundary");
      mk.AddWire(w);
    }
    mk.CheckCompatibility(Standard_True);
    mk.Build();
    if (!mk.IsDone()) return fail("loft failed");
    BRepCheck_Analyzer check(mk.Shape());
    if (!check.IsValid()) return fail("loft made an invalid solid (do the profiles cross?)");
    return put(out, mk.Shape());
  });
}

// Sweeps a face along a path made of connected planar curves (encoded as in curve_edge).
int sk_sweep(const SkShape* face, const double* plane, const int* kinds, const double* data, size_t n, SkShape** out) {
  return guard([&] {
    gp_Dir nd, xd;
    if (!plane_dirs(plane, nd, xd)) return fail("bad path plane");
    TopTools_ListOfShape edges;
    size_t off = 0;
    for (size_t i = 0; i < n; ++i) {
      TopoDS_Edge e;
      bool ok = false;
      if (!curve_edge(kinds[i], data, off, nd, xd, e, ok)) return fail("unknown curve kind");
      if (ok) edges.Append(e);
    }
    if (edges.IsEmpty()) return fail("the path is empty");
    BRepBuilderAPI_MakeWire mw;
    mw.Add(edges);
    if (!mw.IsDone()) return fail("the path isn't one connected chain");
    BRepOffsetAPI_MakePipe mk(mw.Wire(), face->shape);
    mk.Build();
    if (!mk.IsDone()) return fail("sweep failed");
    BRepCheck_Analyzer check(mk.Shape());
    if (!check.IsValid()) return fail("sweep made an invalid solid (is the path too tight for the profile?)");
    return put(out, mk.Shape());
  });
}

// Sweep with its settings: orientation (square to the path, or parallel to the profile), mitred or round corners,
// and a twist (radians) and end scale applied to sections laid along the path, then lofted through. Holes in the
// profile are swept the same way and cut out.
int sk_sweep2(const SkShape* face, const double* plane, const int* kinds, const double* data, size_t n, double twist,
              double scale, int parallel, int round, SkShape** out) {
  return guard([&] {
    gp_Dir nd, xd;
    if (!plane_dirs(plane, nd, xd)) return fail("bad path plane");
    TopTools_ListOfShape edges;
    size_t off = 0;
    for (size_t i = 0; i < n; ++i) {
      TopoDS_Edge e;
      bool ok = false;
      if (!curve_edge(kinds[i], data, off, nd, xd, e, ok)) return fail("unknown curve kind");
      if (ok) edges.Append(e);
    }
    if (edges.IsEmpty()) return fail("the path is empty");
    BRepBuilderAPI_MakeWire mw;
    mw.Add(edges);
    if (!mw.IsDone()) return fail("the path isn't one connected chain");
    const TopoDS_Wire spine = mw.Wire();
    TopExp_Explorer fx(face->shape, TopAbs_FACE);
    if (!fx.More()) return fail("the profile isn't a face");
    const TopoDS_Face prof = TopoDS::Face(fx.Current());
    BRepAdaptor_CompCurve along(spine);
    // The path sampled with the length run so far, to tell how far along it each section is.
    std::vector<std::pair<gp_Pnt, double>> samples;
    {
      const int m = 400;
      double run = 0.0;
      gp_Pnt last = along.Value(along.FirstParameter());
      for (int i = 0; i <= m; ++i) {
        gp_Pnt q = along.Value(along.FirstParameter() + (along.LastParameter() - along.FirstParameter()) * i / m);
        run += q.Distance(last);
        samples.push_back({q, run});
        last = q;
      }
    }
    const double length = samples.back().second;
    auto fraction = [&](const gp_Pnt& c, gp_Vec& dir) {
      size_t best = 0;
      for (size_t i = 1; i < samples.size(); ++i)
        if (samples[i].first.SquareDistance(c) < samples[best].first.SquareDistance(c)) best = i;
      const size_t a = best > 0 ? best - 1 : 0, b = std::min(best + 1, samples.size() - 1);
      dir = gp_Vec(samples[a].first, samples[b].first);
      return length > 0.0 ? samples[best].second / length : 0.0;
    };
    const bool plain = std::abs(twist) < 1e-12 && std::abs(scale - 1.0) < 1e-12;
    auto sweep_wire = [&](const TopoDS_Wire& w, TopoDS_Shape& res) -> bool {
      BRepOffsetAPI_MakePipeShell mk(spine);
      if (parallel) {
        mk.SetMode(gp_Ax2(gp::Origin(), nd, xd));
      } else {
        mk.SetMode(Standard_False);
      }
      mk.SetTransitionMode(round ? BRepBuilderAPI_RoundCorner : BRepBuilderAPI_RightCorner);
      mk.Add(w, Standard_False, Standard_False);
      if (plain) {
        mk.Build();
        if (!mk.IsDone() || !mk.MakeSolid()) return false;
        res = mk.Shape();
        return true;
      }
      const int count = std::clamp(8 + int(std::abs(twist) * 180.0 / M_PI / 10.0) + 4 * edges.Extent(), 8, 96);
      TopTools_ListOfShape secs;
      mk.Simulate(count, secs);
      if (secs.Extent() < 2) return false;
      BRepOffsetAPI_ThruSections loft(Standard_True, Standard_False);
      // Simulate can repeat a section (at the path's end); a repeat would make the loft fail.
      std::optional<gp_Pnt> prev;
      int added = 0;
      for (TopTools_ListIteratorOfListOfShape it(secs); it.More(); it.Next()) {
        TopoDS_Wire sec;
        for (TopExp_Explorer wx(it.Value(), TopAbs_WIRE); wx.More(); wx.Next()) {
          sec = TopoDS::Wire(wx.Current());
          break;
        }
        if (sec.IsNull()) return false;
        GProp_GProps props;
        BRepGProp::LinearProperties(sec, props);
        const gp_Pnt c = props.CentreOfMass();
        if (prev && prev->Distance(c) < 1e-7 * (1.0 + length)) continue;
        prev = c;
        BRepLib_FindSurface fs(sec, -1, Standard_True);
        Handle(Geom_Plane) pl = fs.Found() ? Handle(Geom_Plane)::DownCast(fs.Surface()) : Handle(Geom_Plane)();
        if (pl.IsNull()) return false;
        gp_Dir axis = pl->Axis().Direction();
        // How far along the path, and the same way round in every section: along it.
        gp_Vec tangent;
        const double t = fraction(c, tangent);
        if (tangent.Magnitude() > 1e-12 && gp_Vec(axis).Dot(tangent) < 0.0) axis.Reverse();
        gp_Trsf turn, size;
        turn.SetRotation(gp_Ax1(c, axis), twist * t);
        size.SetScale(c, 1.0 + (scale - 1.0) * t);
        gp_Trsf both = size;
        both.Multiply(turn);
        loft.AddWire(TopoDS::Wire(BRepBuilderAPI_Transform(sec, both, Standard_True).Shape()));
        ++added;
      }
      if (added < 2) return false;
      loft.Build();
      if (!loft.IsDone()) return false;
      res = loft.Shape();
      return true;
    };
    const TopoDS_Wire outer = BRepTools::OuterWire(prof);
    TopoDS_Shape solid;
    if (!sweep_wire(outer, solid)) return fail("sweep failed with these settings");
    for (TopExp_Explorer wx(prof, TopAbs_WIRE); wx.More(); wx.Next()) {
      if (wx.Current().IsSame(outer)) continue;
      TopoDS_Shape hole;
      if (!sweep_wire(TopoDS::Wire(wx.Current()), hole)) return fail("sweep failed for a hole in the profile");
      BRepAlgoAPI_Cut cut(solid, hole);
      if (!cut.IsDone()) return fail("couldn't cut a hole through the sweep");
      solid = cut.Shape();
    }
    BRepCheck_Analyzer check(solid);
    if (!check.IsValid()) return fail("sweep made an invalid solid (is the path too tight for the profile?)");
    return put(out, solid);
  });
}

}  // extern "C"

extern "C" {

// The natural axis of an edge or face: a straight edge's line, a circular edge's center axis, a cylinder's or
// cone's axis, a flat face's normal through its center. *found = 0 when it has none.
int sk_blend_of(const SkShape* s, uint32_t index, double* radius, int* convex, int* found) {
  return guard([&] {
    *found = 0;
    TopoDS_Shape sub;
    if (!sub_shape(s->shape, 1, index, sub)) return fail("no such face");
    TopoDS_Face f = TopoDS::Face(sub);
    double r = blend_radius(f);
    if (r <= 0.0) return 0;
    // Smooth on two sides (a hole's wall or a boss isn't).
    TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(s->shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    int smooth = 0;
    for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
      TopoDS_Edge e = TopoDS::Edge(ex.Current());
      if (!edge_faces.Contains(e) || BRep_Tool::Degenerated(e)) continue;
      for (TopTools_ListIteratorOfListOfShape it(edge_faces.FindFromKey(e)); it.More(); it.Next()) {
        if (!it.Value().IsSame(f) && tangent_along(e, f, TopoDS::Face(it.Value()))) ++smooth;
      }
    }
    if (smooth < 2) return 0;
    // Convex when the face's outward normal points away from the curvature center.
    BRepGProp_Face gf(f);
    double u0, u1, v0, v1;
    gf.Bounds(u0, u1, v0, v1);
    gp_Pnt p;
    gp_Vec n;
    gf.Normal((u0 + u1) / 2, (v0 + v1) / 2, p, n);
    BRepAdaptor_Surface sf(f);
    gp_Pnt c;
    if (sf.GetType() == GeomAbs_Cylinder) {
      gp_Ax1 ax = sf.Cylinder().Axis();
      gp_Vec v(ax.Location(), p);
      c = ax.Location().Translated(gp_Vec(ax.Direction()) * v.Dot(gp_Vec(ax.Direction())));
    } else {
      gp_Torus t = sf.Torus();
      gp_Ax1 ax = t.Axis();
      gp_Vec v(ax.Location(), p);
      gp_Vec along = gp_Vec(ax.Direction()) * v.Dot(gp_Vec(ax.Direction()));
      gp_Vec radial = v - along;
      if (radial.Magnitude() < 1e-12) return 0;
      c = ax.Location().Translated(radial.Normalized() * t.MajorRadius());
    }
    *radius = r;
    *convex = n.Dot(gp_Vec(c, p)) > 0.0 ? 1 : 0;
    *found = 1;
    return 0;
  });
}

int sk_axis_of(const SkShape* s, int kind, uint32_t index, double* out, int* found) {
  return guard([&] {
    *found = 0;
    TopoDS_Shape sub;
    if (kind == 0 || !sub_shape(s->shape, kind, index, sub)) return fail("no such edge or face");
    gp_Ax1 ax;
    bool ok = false;
    if (kind == 2) {
      BRepAdaptor_Curve c(TopoDS::Edge(sub));
      if (c.GetType() == GeomAbs_Line) {
        gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
        if (a.Distance(b) > 1e-9) {
          ax = gp_Ax1(a, gp_Dir(gp_Vec(a, b)));
          ok = true;
        }
      } else if (c.GetType() == GeomAbs_Circle) {
        ax = c.Circle().Axis();
        ok = true;
      } else {
        // A non-uniform scale leaves straight edges as B-splines: still a line if every sample lies on the chord.
        double u0 = c.FirstParameter(), u1 = c.LastParameter();
        gp_Pnt a = c.Value(u0), b = c.Value(u1);
        double len = a.Distance(b);
        if (len > 1e-9) {
          gp_Lin chord(a, gp_Dir(gp_Vec(a, b)));
          bool straight = true;
          for (int i = 1; i < 8 && straight; ++i) straight = chord.Distance(c.Value(u0 + (u1 - u0) * i / 8.0)) < 1e-7 * (1.0 + len);
          if (straight) {
            ax = gp_Ax1(a, chord.Direction());
            ok = true;
          }
        }
      }
    } else {
      BRepAdaptor_Surface f(TopoDS::Face(sub));
      if (f.GetType() == GeomAbs_Cylinder) {
        ax = f.Cylinder().Axis();
        ok = true;
      } else if (f.GetType() == GeomAbs_Cone) {
        ax = f.Cone().Axis();
        ok = true;
      } else if (f.GetType() == GeomAbs_Plane) {
        GProp_GProps props;
        BRepGProp::SurfaceProperties(sub, props);
        gp_Dir n = f.Plane().Axis().Direction();
        if (TopoDS::Face(sub).Orientation() == TopAbs_REVERSED) n.Reverse();
        ax = gp_Ax1(props.CentreOfMass(), n);
        ok = true;
      }
    }
    if (!ok) return 0;
    out[0] = ax.Location().X(); out[1] = ax.Location().Y(); out[2] = ax.Location().Z();
    out[3] = ax.Direction().X(); out[4] = ax.Direction().Y(); out[5] = ax.Direction().Z();
    *found = 1;
    return 0;
  });
}

}  // extern "C"

extern "C" {

// The edges bounding a face, as indices in the shape's edge map (the ids picking and fillets use).
int sk_face_edges(const SkShape* s, uint32_t face, uint32_t** out, size_t* n) {
  return guard([&] {
    *out = nullptr;
    *n = 0;
    TopoDS_Face f;
    if (!face_at(s->shape, face, f)) return fail("no such face");
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    std::vector<uint32_t> ids;
    for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
      int i = edges.FindIndex(ex.Current());
      if (i > 0) {
        uint32_t id = uint32_t(i - 1);
        bool seen = false;
        for (uint32_t x : ids) seen = seen || x == id;
        if (!seen) ids.push_back(id);
      }
    }
    if (ids.empty()) return 0;
    auto arr = new uint32_t[ids.size()];
    for (size_t i = 0; i < ids.size(); ++i) arr[i] = ids[i];
    *out = arr;
    *n = ids.size();
    return 0;
  });
}

void sk_u32_array_free(uint32_t* arr) { delete[] arr; }

}  // extern "C"

extern "C" {

// An edge's exact curve. kind 0 line (fixed: a, b), 1 full circle (center, normal, radius), 2 circular arc
// (a, mid, b), 3 anything else (*pts: npts sampled points, xyz each; free with sk_f64_array_free).
int sk_edge_curve(const SkShape* s, uint32_t edge, int* kind, double* fixed, double** pts, size_t* npts) {
  return guard([&] {
    *pts = nullptr;
    *npts = 0;
    TopoDS_Shape sub;
    if (!sub_shape(s->shape, 2, edge, sub)) return fail("no such edge");
    BRepAdaptor_Curve c(TopoDS::Edge(sub));
    double t0 = c.FirstParameter(), t1 = c.LastParameter();
    auto put3 = [&](int at, const gp_XYZ& p) { fixed[at] = p.X(); fixed[at + 1] = p.Y(); fixed[at + 2] = p.Z(); };
    if (c.GetType() == GeomAbs_Line) {
      *kind = 0;
      put3(0, c.Value(t0).XYZ());
      put3(3, c.Value(t1).XYZ());
      return 0;
    }
    if (c.GetType() == GeomAbs_Circle) {
      gp_Circ circ = c.Circle();
      if (std::abs(t1 - t0 - 2.0 * std::acos(-1.0)) < 1e-9 || c.Value(t0).Distance(c.Value(t1)) < 1e-9) {
        *kind = 1;
        put3(0, circ.Location().XYZ());
        put3(3, circ.Axis().Direction().XYZ());
        fixed[6] = circ.Radius();
      } else {
        *kind = 2;
        put3(0, c.Value(t0).XYZ());
        put3(3, c.Value((t0 + t1) / 2.0).XYZ());
        put3(6, c.Value(t1).XYZ());
      }
      return 0;
    }
    *kind = 3;
    const int n = 48;
    auto arr = new double[3 * (n + 1)];
    for (int i = 0; i <= n; ++i) {
      gp_Pnt p = c.Value(t0 + (t1 - t0) * double(i) / n);
      arr[3 * i] = p.X(); arr[3 * i + 1] = p.Y(); arr[3 * i + 2] = p.Z();
    }
    *pts = arr;
    *npts = n + 1;
    return 0;
  });
}

void sk_f64_array_free(double* arr) { delete[] arr; }

// Fill: a smooth surface through a closed loop of the shape's edges (C0 to them), sewn onto the shape; a shell
// that closes up becomes a solid (a hole in a mesh or a missing face filled).
int sk_fill(const SkShape* s, const uint32_t* ids, size_t n, SkShape** out) {
  return guard([&] {
    if (n == 0) return fail("pick the edges around the opening");
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    BRepOffsetAPI_MakeFilling fill;
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      fill.Add(TopoDS::Edge(edges(int(ids[i]) + 1)), GeomAbs_C0, Standard_True);
    }
    fill.Build();
    if (!fill.IsDone()) return fail("those edges don't close a loop to fill");
    TopoDS_Shape patch = fill.Shape();
    BRepBuilderAPI_Sewing sew(1e-3);
    sew.Add(s->shape);
    sew.Add(patch);
    sew.Perform();
    TopoDS_Shape sewn = sew.SewedShape();
    if (sewn.IsNull()) return fail("the filled face couldn't be joined to the body");
    // A shell that now closes becomes a solid.
    for (TopExp_Explorer ex(sewn, TopAbs_SHELL); ex.More(); ex.Next()) {
      TopoDS_Shell shell = TopoDS::Shell(ex.Current());
      if (!BRep_Tool::IsClosed(shell)) continue;
      BRepBuilderAPI_MakeSolid mk(shell);
      if (!mk.IsDone()) continue;
      ShapeFix_Solid fix;
      fix.Init(mk.Solid());
      fix.Perform();
      TopoDS_Shape solid = fix.Solid();
      if (!solid.IsNull() && BRepCheck_Analyzer(solid).IsValid()) return put(out, solid);
    }
    return put(out, sewn);
  });
}

}  // extern "C"

extern "C" {

// Extrudes a planar face along (dx, dy, dz) (which must be along its normal, either way), with the side walls
// leaning in by `angle` radians (out for a negative angle).
int sk_prism_draft(const SkShape* face, double dx, double dy, double dz, double angle, SkShape** out) {
  return guard([&] {
    TopExp_Explorer ex(face->shape, TopAbs_FACE);
    if (!ex.More()) return fail("not a face");
    TopoDS_Face f = TopoDS::Face(ex.Current());
    gp_Vec dir(dx, dy, dz);
    double height = dir.Magnitude();
    if (height <= 0.0) return fail("extrude distance is zero");
    gp_Pnt o;
    gp_Dir n;
    if (!plane_of(f, o, n)) return fail("only flat faces can be extruded");
    // LocOpe measures the height along the leaning wall; scale it so the cap lands at `height`.
    if (std::abs(gp_Vec(n).Dot(dir)) < height * (1.0 - 1e-6)) return fail("a drafted extrude goes straight out of the face");
    LocOpe_DPrism dp(f, height / std::cos(angle), angle);
    TopoDS_Shape r = dp.Shape();
    if (r.IsNull()) return fail("draft extrude failed");
    // It grows along the face's own normal; mirror it through the face when that's the wrong way.
    double along = 0.0;
    {
      Bnd_Box b;
      BRepBndLib::Add(r, b);
      double x0, y0, z0, x1, y1, z1;
      b.Get(x0, y0, z0, x1, y1, z1);
      gp_Vec c(o, gp_Pnt((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0));
      along = c.Dot(dir);
    }
    if (along < 0.0) {
      gp_Trsf m;
      m.SetMirror(gp_Ax2(o, n));
      r = BRepBuilderAPI_Transform(r, m, Standard_True).Shape();
    }
    BRepCheck_Analyzer check(r);
    if (!check.IsValid()) {
      ShapeFix_Solid fix;
      TopExp_Explorer sh(r, TopAbs_SHELL);
      if (!sh.More()) return fail("draft extrude made no solid");
      fix.Init(BRepBuilderAPI_MakeSolid(TopoDS::Shell(sh.Current())).Solid());
      fix.Perform();
      r = fix.Solid();
      if (r.IsNull() || !BRepCheck_Analyzer(r).IsValid()) return fail("draft angle too steep for this height");
    }
    if (volume_of(r) < 0.0) return fail("draft extrude came out inside out");
    return put(out, r);
  });
}

}  // extern "C"

extern "C" {

int sk_edge_count(const SkShape* s, uint32_t* n) {
  return guard([&] {
    TopTools_IndexedMapOfShape map;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, map);
    *n = uint32_t(map.Extent());
    return 0;
  });
}

namespace {
int offset_result(BRepOffsetAPI_MakeOffset& mk, double distance, SkShape** out) {
  mk.Perform(distance);
  if (!mk.IsDone()) return fail("offset failed (too far for this shape?)");
  TopoDS_Shape r = mk.Shape();
  if (r.IsNull()) return fail("offset made nothing");
  return put(out, r);
}
}  // namespace

// A 2D offset of a connected chain of planar curves (encoded as in curve_edge), corners joined sharp. Open
// chains stay open. Returns the offset edges as one shape.
int sk_offset_curves(const double* plane, const int* kinds, const double* data, size_t n, double distance, SkShape** out) {
  return guard([&] {
    gp_Dir nd, xd;
    if (!plane_dirs(plane, nd, xd)) return fail("bad sketch plane");
    TopTools_ListOfShape edges;
    size_t off = 0;
    for (size_t i = 0; i < n; ++i) {
      TopoDS_Edge e;
      bool ok = false;
      if (!curve_edge(kinds[i], data, off, nd, xd, e, ok)) return fail("unknown curve kind");
      if (ok) edges.Append(e);
    }
    if (edges.IsEmpty()) return fail("nothing to offset");
    BRepBuilderAPI_MakeWire mw;
    mw.Add(edges);
    if (!mw.IsDone()) return fail("the curves aren't one connected chain");
    TopoDS_Wire w = mw.Wire();
    gp_Pln pln(gp_Ax3(gp_Pnt(plane[0], plane[1], plane[2]), nd, xd));
    BRepBuilderAPI_MakeFace base(pln);
    if (!base.IsDone()) return fail("bad sketch plane");
    BRepOffsetAPI_MakeOffset mk;
    mk.Init(base.Face(), GeomAbs_Intersection, !BRep_Tool::IsClosed(w));
    mk.AddWire(w);
    return offset_result(mk, distance, out);
  });
}

// A 2D offset of all of a planar face's boundaries (outwards when positive), corners joined sharp.
int sk_offset_face_outline(const SkShape* face, double distance, SkShape** out) {
  return guard([&] {
    TopExp_Explorer ex(face->shape, TopAbs_FACE);
    if (!ex.More()) return fail("not a face");
    BRepOffsetAPI_MakeOffset mk(TopoDS::Face(ex.Current()), GeomAbs_Intersection);
    return offset_result(mk, distance, out);
  });
}

}  // extern "C"

struct SkLines {
  std::vector<double> xy;        // all points, x y pairs
  std::vector<uint32_t> lens;    // points per polyline
  std::vector<uint8_t> hidden;   // 1 when the polyline is behind something
};

extern "C" {

// Hidden-line projection of shapes onto a view plane: frame = origin, direction toward the viewer, x axis
// (9 doubles). Polylines are in view-plane millimeters.
int sk_hlr(const SkShape* const* shapes, size_t n, const double* frame, double deflection, SkLines** out) {
  return guard([&] {
    *out = nullptr;
    gp_Vec d(frame[3], frame[4], frame[5]), x(frame[6], frame[7], frame[8]);
    if (d.Magnitude() <= 0.0 || x.Magnitude() <= 0.0 || d.IsParallel(x, 1e-9)) return fail("bad view direction");
    Handle(HLRBRep_Algo) algo = new HLRBRep_Algo();
    for (size_t i = 0; i < n; ++i) algo->Add(shapes[i]->shape);
    HLRAlgo_Projector proj(gp_Ax2(gp_Pnt(frame[0], frame[1], frame[2]), gp_Dir(d), gp_Dir(x)));
    algo->Projector(proj);
    algo->Update();
    algo->Hide();
    HLRBRep_HLRToShape h(algo);
    auto lines = new SkLines();
    auto add = [&](const TopoDS_Shape& comp, uint8_t hidden) {
      if (comp.IsNull()) return;
      for (TopExp_Explorer ex(comp, TopAbs_EDGE); ex.More(); ex.Next()) {
        BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
        GCPnts_TangentialDeflection pts(c, 0.1, deflection);
        int m = pts.NbPoints();
        if (m < 2) continue;
        for (int k = 1; k <= m; ++k) {
          gp_Pnt p = pts.Value(k);
          lines->xy.push_back(p.X());
          lines->xy.push_back(p.Y());
        }
        lines->lens.push_back(uint32_t(m));
        lines->hidden.push_back(hidden);
      }
    };
    add(h.VCompound(), 0);
    add(h.OutLineVCompound(), 0);
    add(h.HCompound(), 1);
    add(h.OutLineHCompound(), 1);
    *out = lines;
    return 0;
  });
}

void sk_lines_free(SkLines* l) { delete l; }
size_t sk_lines_count(const SkLines* l) { return l->lens.size(); }
const uint32_t* sk_lines_lens(const SkLines* l) { return l->lens.data(); }
const uint8_t* sk_lines_hidden(const SkLines* l) { return l->hidden.data(); }
size_t sk_lines_points(const SkLines* l) { return l->xy.size() / 2; }
const double* sk_lines_xy(const SkLines* l) { return l->xy.data(); }

}  // extern "C"

// ---------------------------------------------------------------- Wrap & Emboss
#include <Geom_CylindricalSurface.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GCE2d_MakeSegment.hxx>
#include <TColgp_HArray1OfPnt2d.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <ShapeFix_Face.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Cone.hxx>

namespace {

// How a sketch plane's points land on a round face: kept lengths (arc length round the axis, length along it),
// so the profile isn't stretched (the manual's "maintains the surface-area dimensions").
struct Wrapper {
  bool cone = false;
  double radius = 0, angle = 0;  // cylinder radius; cone half-angle (the cone is ours: apex at the origin)
  gp_Pnt center;                 // the profile's centre, on the sketch plane
  gp_Vec along, across;          // in the plane: round the axis, and towards the axis direction
  double rot_c = 1, rot_s = 0;   // the profile's rotation about its centre
  double u0 = 0, v0 = 0;         // where the centre lands (surface parameters)
  double v_shift = 0;            // the skin cone's slant parameter against the face's (it sits a little apart)
  Handle(Geom_Surface) surface;

  gp_Pnt2d uv(const gp_Pnt& w) const {
    gp_Vec d(center, w);
    double s0 = d.Dot(along), t0 = d.Dot(across);
    double s = s0 * rot_c - t0 * rot_s, t = s0 * rot_s + t0 * rot_c;
    if (!cone) return gp_Pnt2d(u0 + s / radius, v0 + t);
    // A cone unrolls flat into a sector: the centre sits at slant distance v0 from the apex, angle 0.
    double x = v0 + t, y = s;
    double rho = std::sqrt(x * x + y * y);
    return gp_Pnt2d(u0 + std::atan2(y, x) / std::sin(angle), rho + v_shift);
  }
};

// The face's points as the wrapper maps them, sampled along each edge in wire order.
bool wrap_wire(const TopoDS_Wire& wire, const TopoDS_Face& on, const Wrapper& w, TopoDS_Wire& out) {
  BRepBuilderAPI_MakeWire mw;
  for (BRepTools_WireExplorer ex(wire, on); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = ex.Current();
    BRepAdaptor_Curve c(e);
    double a = c.FirstParameter(), b = c.LastParameter();
    if (e.Orientation() == TopAbs_REVERSED) std::swap(a, b);
    Handle(Geom2d_Curve) c2;
    if (!w.cone && c.GetType() == GeomAbs_Line) {
      gp_Pnt2d p = w.uv(c.Value(a)), q = w.uv(c.Value(b));
      if (p.Distance(q) < 1e-9) continue;
      c2 = GCE2d_MakeSegment(p, q).Value();
    } else {
      GProp_GProps g;
      BRepGProp::LinearProperties(e, g);
      int n = std::max(16, std::min(400, int(g.Mass() / 0.25) + 8));
      Handle(TColgp_HArray1OfPnt2d) pts = new TColgp_HArray1OfPnt2d(1, n);
      for (int i = 0; i < n; ++i) pts->SetValue(i + 1, w.uv(c.Value(a + (b - a) * i / (n - 1))));
      bool closed = pts->Value(1).Distance(pts->Value(n)) < 1e-9;
      if (closed) {
        // A whole circle: interpolate it as a periodic curve through all but the repeated end.
        Handle(TColgp_HArray1OfPnt2d) open = new TColgp_HArray1OfPnt2d(1, n - 1);
        for (int i = 1; i < n; ++i) open->SetValue(i, pts->Value(i));
        Geom2dAPI_Interpolate ip(open, Standard_True, 1e-9);
        ip.Perform();
        if (!ip.IsDone()) return false;
        c2 = ip.Curve();
      } else {
        Geom2dAPI_Interpolate ip(pts, Standard_False, 1e-9);
        ip.Perform();
        if (!ip.IsDone()) return false;
        c2 = ip.Curve();
      }
    }
    BRepBuilderAPI_MakeEdge me(c2, w.surface, c2->FirstParameter(), c2->LastParameter());
    if (!me.IsDone()) return false;
    mw.Add(me.Edge());
    if (!mw.IsDone()) return false;
  }
  if (!mw.IsDone()) return false;
  out = mw.Wire();
  BRepLib::BuildCurves3d(out);
  return true;
}

}  // namespace

extern "C" {

// Wraps flat sketch regions onto a body's cylindrical or conical face (lengths kept) and raises them by `depth`
// (negative engraves). `face` < 0 picks the round face nearest the profiles. `frame` = plane origin, x, y.
// `rotation` turns the profiles about their centre (radians); `center` (3 numbers, or null) is the point on the
// face the profiles' centre goes to (default: the face round from the profile, facing the sketch).
int sk_wrap_emboss(const SkShape* body, int64_t face, const SkShape* const* regions, size_t n, const double* frame, double depth,
                   double rotation, const double* center, SkShape** out) {
  return guard([&] {
    if (n == 0) return fail("pick sketch profiles to wrap");
    if (!(std::abs(depth) > 1e-7)) return fail("the emboss depth must not be zero");
    gp_Vec x(frame[3], frame[4], frame[5]), y(frame[6], frame[7], frame[8]);
    gp_Vec nrm = x.Crossed(y);
    if (nrm.Magnitude() < 1e-12) return fail("bad sketch plane");
    nrm.Normalize();
    Bnd_Box box;
    for (size_t i = 0; i < n; ++i) BRepBndLib::Add(regions[i]->shape, box);
    if (box.IsVoid()) return fail("empty profiles");
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    gp_Pnt pc((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2);

    TopoDS_Face target;
    if (face >= 0) {
      if (!face_at(body->shape, uint32_t(face), target)) return fail("no such face");
    } else {
      // The round face nearest the profiles' centre.
      double best = 1e300;
      TopoDS_Vertex v = BRepBuilderAPI_MakeVertex(pc);
      for (TopExp_Explorer ex(body->shape, TopAbs_FACE); ex.More(); ex.Next()) {
        TopoDS_Face f = TopoDS::Face(ex.Current());
        GeomAbs_SurfaceType t = BRepAdaptor_Surface(f).GetType();
        if (t != GeomAbs_Cylinder && t != GeomAbs_Cone) continue;
        BRepExtrema_DistShapeShape d(v, f);
        if (d.IsDone() && d.Value() < best) {
          best = d.Value();
          target = f;
        }
      }
      if (target.IsNull()) return fail("the body has no cylindrical or conical face");
    }
    BRepAdaptor_Surface sa(target);
    Wrapper w;
    gp_Pnt loc;
    gp_Dir zd;
    if (sa.GetType() == GeomAbs_Cylinder) {
      gp_Cylinder c = sa.Cylinder();
      loc = c.Location();
      zd = c.Axis().Direction();
      w.radius = c.Radius();
    } else if (sa.GetType() == GeomAbs_Cone) {
      gp_Cone k = sa.Cone();
      w.cone = true;
      w.angle = std::abs(k.SemiAngle());
      loc = k.Apex();
      // Our cone grows along +z from its apex.
      zd = k.SemiAngle() > 0 ? k.Axis().Direction() : k.Axis().Direction().Reversed();
    } else {
      return fail("Wrap & Emboss needs a cylindrical or conical face");
    }
    gp_Vec z(zd);
    gp_Vec side = nrm - z * nrm.Dot(z);
    if (side.Magnitude() < 1e-6) return fail("the sketch faces along the axis: draw it on a plane beside the face");
    gp_Dir d0(side);
    gp_Dir t = zd.Crossed(d0);
    w.along = gp_Vec(t);
    gp_Vec across = nrm.Crossed(w.along);
    if (across.Dot(z) < 0) across.Reverse();
    w.across = across.Normalized();
    w.center = pc;
    w.rot_c = std::cos(rotation);
    w.rot_s = std::sin(rotation);
    // The skin starts a little under the face (above it for an engraving), so the boolean meets a real overlap
    // rather than two surfaces lying on each other; the profile is still measured on the face itself.
    double sink = std::max(1e-3, std::min(0.25 * std::abs(depth), 0.05 * (w.cone ? 1.0 : w.radius)));
    double sgn = depth > 0 ? 1.0 : -1.0;
    double thick = sgn * (std::abs(depth) + sink);
    double skin_r = w.radius - sgn * sink;
    if (!w.cone && !(skin_r > 1e-6)) return fail("the face is too small for that engraving");
    gp_Pnt skin_apex = loc.Translated(gp_Vec(zd) * (sgn * sink / std::sin(w.cone ? w.angle : 1.0)));
    gp_Vec dc(loc, center ? gp_Pnt(center[0], center[1], center[2]) : pc);
    if (!w.cone) {
      w.surface = new Geom_CylindricalSurface(gp_Ax3(loc, zd, d0), skin_r);
      // A given centre is a point on the face (its angle round the axis); else the profile's own offset across
      // the axis, as arc length, from the side facing the sketch.
      w.u0 = center ? std::atan2(dc.Dot(gp_Vec(t)), dc.Dot(gp_Vec(d0))) : dc.Dot(gp_Vec(t)) / w.radius;
      w.v0 = dc.Dot(z);
    } else {
      w.surface = new Geom_ConicalSurface(gp_Ax3(skin_apex, zd, d0), w.angle, 0.0);
      w.v_shift = -sgn * sink * std::cos(w.angle) / std::sin(w.angle);
      double rho = dc.Dot(z) / std::cos(w.angle);
      if (!(rho > 1e-6)) return fail("the profile sits beyond the cone's tip");
      w.v0 = rho;
      double r = rho * std::sin(w.angle);
      w.u0 = center ? std::atan2(dc.Dot(gp_Vec(t)), dc.Dot(gp_Vec(d0))) : dc.Dot(gp_Vec(t)) / r;
    }

    TopoDS_Shape result = body->shape;
    for (size_t i = 0; i < n; ++i) {
      for (TopExp_Explorer ex(regions[i]->shape, TopAbs_FACE); ex.More(); ex.Next()) {
        TopoDS_Face flat = TopoDS::Face(ex.Current());
        TopoDS_Wire outer = BRepTools::OuterWire(flat);
        TopoDS_Wire wrapped;
        if (outer.IsNull() || !wrap_wire(outer, flat, w, wrapped)) return fail("couldn't wrap that profile");
        BRepBuilderAPI_MakeFace mf(w.surface, wrapped, Standard_True);
        if (!mf.IsDone()) return fail("couldn't wrap that profile onto the face");
        for (TopExp_Explorer wx(flat, TopAbs_WIRE); wx.More(); wx.Next()) {
          TopoDS_Wire hole = TopoDS::Wire(wx.Current());
          if (hole.IsSame(outer)) continue;
          TopoDS_Wire wh;
          if (!wrap_wire(hole, flat, w, wh)) return fail("couldn't wrap a hole of that profile");
          mf.Add(wh);
        }
        ShapeFix_Face fix(mf.Face());
        fix.Perform();
        TopoDS_Face skin = fix.Face();
        BRepLib::BuildCurves3d(skin);
        auto thicken = [&](double d) {
          BRepOffsetAPI_MakeThickSolid mk;
          mk.MakeThickSolidBySimple(skin, d);
          mk.Build();
          if (!mk.IsDone()) return TopoDS_Shape();
          // The offset of a lone face can come out inside out.
          TopoDS_Shape r = mk.Shape();
          if (volume_of(r) < 0) r.Reverse();
          return r;
        };
        TopoDS_Shape solid = thicken(thick);
        if (solid.IsNull()) return fail("couldn't raise the wrapped profile");
        // Raised material sits outside the face, an engraving inside: flip if the face came out turned over.
        GProp_GProps g;
        BRepGProp::VolumeProperties(solid, g);
        gp_Vec rel(w.cone ? skin_apex : loc, g.CentreOfMass());
        double off = (rel - z * rel.Dot(z)).Magnitude();
        double surf_r = w.cone ? rel.Dot(z) * std::tan(w.angle) : skin_r;
        if ((depth > 0) != (off > surf_r)) {
          solid = thicken(-thick);
          if (solid.IsNull()) return fail("couldn't raise the wrapped profile");
        }
        if (depth > 0) {
          BRepAlgoAPI_Fuse op(result, solid);
          if (!op.IsDone() || op.HasErrors()) return fail("the emboss couldn't join the body");
          op.SimplifyResult();
          result = op.Shape();
        } else {
          BRepAlgoAPI_Cut op(result, solid);
          if (!op.IsDone() || op.HasErrors()) return fail("the engraving couldn't cut the body");
          op.SimplifyResult();
          result = op.Shape();
        }
      }
    }
    BRepCheck_Analyzer check(result);
    if (!check.IsValid()) return fail("Wrap & Emboss made an invalid body");
    return put(out, result);
  });
}

}  // extern "C"

// ---------------------------------------------------------------- Fillet sizes and corners
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepLProp_SLProps.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_ListOfShape.hxx>

namespace {

// Outward normal of `f` at the point of it nearest `p`.
bool normal_near(const TopoDS_Face& f, const gp_Pnt& p, gp_Dir& n) {
  Handle(Geom_Surface) g = BRep_Tool::Surface(f);
  if (g.IsNull()) return false;
  ShapeAnalysis_Surface sas(g);
  gp_Pnt2d uv = sas.ValueOfUV(p, 1e-7);
  BRepLProp_SLProps props(BRepAdaptor_Surface(f), uv.X(), uv.Y(), 1, 1e-9);
  if (!props.IsNormalDefined()) return false;
  n = props.Normal();
  if (f.Orientation() == TopAbs_REVERSED) n.Reverse();
  return true;
}

// The radius that gives a fillet of chord `width` along `e`: the chord of a circle meeting two faces whose normals
// are θ apart is 2 r sin(θ / 2). Measured at points along the edge (relative parameter 0..1, ends inset a little):
// `law` gets (t, r) at each, `r` their mean. Where θ changes along the edge the law keeps the chord constant.
bool chordal_radius(const TopoDS_Shape& s, const TopoDS_Edge& e, double width, double& r, std::vector<std::pair<double, double>>& law) {
  TopTools_IndexedDataMapOfShapeListOfShape ef;
  TopExp::MapShapesAndAncestors(s, TopAbs_EDGE, TopAbs_FACE, ef);
  int k = ef.FindIndex(e);
  if (k == 0 || ef(k).Extent() != 2) return false;
  TopoDS_Face fa = TopoDS::Face(ef(k).First()), fb = TopoDS::Face(ef(k).Last());
  BRepAdaptor_Curve c(e);
  double sum = 0;
  law.clear();
  const int steps = 12;
  for (int i = 0; i <= steps; ++i) {
    double t = std::min(std::max(double(i) / steps, 1e-3), 1.0 - 1e-3);
    gp_Pnt p = c.Value(c.FirstParameter() + (c.LastParameter() - c.FirstParameter()) * t);
    gp_Dir na, nb;
    if (!normal_near(fa, p, na) || !normal_near(fb, p, nb)) continue;
    double theta = na.Angle(nb);
    if (theta < 1e-3) continue;
    double ri = width / (2.0 * std::sin(theta / 2.0));
    sum += ri;
    law.push_back({double(i) / steps, ri});
  }
  if (law.empty()) return false;
  r = sum / double(law.size());
  return true;
}

}  // namespace

extern "C" {

// Fillets `ids` with `size` as the radius, or as the chord width (`chordal`). `setback` > 0 gives Setback corners:
// where three or more of the rounded edges meet, the corner is cut back to `setback` from the vertex and closed
// with one smooth patch, tangent to the faces around it, instead of the rolling ball.
int sk_fillet_edges_ex(const SkShape* s, const uint32_t* ids, size_t n, double size, int chordal, double setback, SkShape** out) {
  return guard([&] {
    TopTools_IndexedMapOfShape edges;
    TopExp::MapShapes(s->shape, TopAbs_EDGE, edges);
    BRepFilletAPI_MakeFillet mk(s->shape);
    std::vector<TopoDS_Edge> picked;
    double largest = 0;
    for (size_t i = 0; i < n; ++i) {
      if (ids[i] >= uint32_t(edges.Extent())) return fail("no such edge");
      TopoDS_Edge e = TopoDS::Edge(edges(int(ids[i]) + 1));
      double r = size;
      std::vector<std::pair<double, double>> law;
      if (chordal && !chordal_radius(s->shape, e, size, r, law)) return fail("a chordal fillet needs an edge between two faces");
      double lo = r, hi = r;
      for (const auto& [t, ri] : law) {
        lo = std::min(lo, ri);
        hi = std::max(hi, ri);
      }
      // The angle between the faces changes along the edge: the radius follows it, so the chord stays the same.
      if (chordal && law.size() >= 2 && law.front().first == 0.0 && law.back().first == 1.0 && hi - lo > 1e-6 * hi) {
        TColgp_Array1OfPnt2d uandr(1, int(law.size()));
        for (size_t j = 0; j < law.size(); ++j) uandr.SetValue(int(j) + 1, gp_Pnt2d(law[j].first, law[j].second));
        mk.Add(uandr, e);
      } else {
        mk.Add(r, e);
      }
      picked.push_back(e);
      largest = std::max(largest, hi);
    }
    mk.Build();
    if (!mk.IsDone()) return fail("fillet failed (radius too large?)");
    TopoDS_Shape rolled = mk.Shape();
    if (!(setback > 0)) return put_checked(out, rolled, s->shape, "fillet failed (radius too large?)", true);

    // Corners: vertices where three or more picked edges meet.
    TopTools_IndexedDataMapOfShapeListOfShape ve;
    TopExp::MapShapesAndAncestors(s->shape, TopAbs_VERTEX, TopAbs_EDGE, ve);
    std::vector<std::pair<gp_Pnt, double>> corners;  // the vertex, and its shortest rounded edge
    for (int i = 1; i <= ve.Extent(); ++i) {
      // Distinct picked edges at the vertex (the ancestor list may hold an edge once per face it bounds).
      int count = 0;
      double shortest = 1e300;
      for (const TopoDS_Edge& e : picked) {
        bool here = false;
        for (TopTools_ListIteratorOfListOfShape it(ve(i)); it.More() && !here; it.Next()) here = e.IsSame(it.Value());
        if (!here) continue;
        ++count;
        GProp_GProps g;
        BRepGProp::LinearProperties(e, g);
        shortest = std::min(shortest, g.Mass());
      }
      if (count >= 3) corners.push_back({BRep_Tool::Pnt(TopoDS::Vertex(ve.FindKey(i))), shortest});
    }
    if (corners.empty()) return put_checked(out, rolled, s->shape, "fillet failed (radius too large?)", true);
    TopoDS_Shape body = rolled;
    for (const auto& [c, shortest] : corners) {
      // Back from the vertex by `setback`, past the rolling ball, short of the next corner's patch.
      double d = std::min(std::max(setback, largest * 1.2), 0.45 * shortest);
      if (d < largest * 1.05) return fail("the edges are too short for setback corners at that size");
      // Cut a ball out of the corner, then put a smooth patch where the ball's face was.
      TopoDS_Shape ball = BRepPrimAPI_MakeSphere(c, d).Shape();
      BRepAlgoAPI_Cut cut(body, ball);
      if (!cut.IsDone() || cut.HasErrors()) return fail("the setback corner couldn't be cut");
      TopoDS_Shape cutted = cut.Shape();
      BRepOffsetAPI_MakeFilling fill;
      std::vector<TopoDS_Face> keep;
      int bounds = 0;
      TopTools_IndexedDataMapOfShapeListOfShape cef;
      TopExp::MapShapesAndAncestors(cutted, TopAbs_EDGE, TopAbs_FACE, cef);
      for (TopExp_Explorer fx(cutted, TopAbs_FACE); fx.More(); fx.Next()) {
        TopoDS_Face f = TopoDS::Face(fx.Current());
        BRepAdaptor_Surface sa(f);
        bool on_ball = sa.GetType() == GeomAbs_Sphere && sa.Sphere().Location().Distance(c) < 1e-6 &&
                       std::abs(sa.Sphere().Radius() - d) < 1e-6;
        if (!on_ball) {
          keep.push_back(f);
          continue;
        }
        // The ball face's edges bound the patch; each is tangent to the face across it.
        for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
          TopoDS_Edge e = TopoDS::Edge(ex.Current());
          int k = cef.FindIndex(e);
          if (k == 0) continue;
          TopoDS_Face other;
          for (TopTools_ListIteratorOfListOfShape it(cef(k)); it.More(); it.Next())
            if (!it.Value().IsSame(f)) other = TopoDS::Face(it.Value());
          if (other.IsNull()) fill.Add(e, GeomAbs_C0);
          else fill.Add(e, other, GeomAbs_G1);
          ++bounds;
        }
      }
      if (bounds < 3) return fail("the setback corner has no clean outline");
      fill.Build();
      if (!fill.IsDone()) return fail("the setback corner couldn't be filled");
      BRepBuilderAPI_Sewing sew(1e-4);
      for (const TopoDS_Face& f : keep) sew.Add(f);
      sew.Add(fill.Shape());
      sew.Perform();
      TopoDS_Shape sewn = sew.SewedShape();
      TopoDS_Shell shell;
      int shells = 0;
      for (TopExp_Explorer sx(sewn, TopAbs_SHELL); sx.More(); sx.Next()) {
        shell = TopoDS::Shell(sx.Current());
        ++shells;
      }
      if (shells != 1) return fail("the setback corner didn't close up");
      ShapeFix_Solid fix;
      fix.Init(BRepBuilderAPI_MakeSolid(shell).Solid());
      fix.Perform();
      body = fix.Solid();
      if (volume_of(body) < 0) body.Reverse();
    }
    return put_checked(out, body, s->shape, "setback corners failed", true);
  });
}

}  // extern "C"
