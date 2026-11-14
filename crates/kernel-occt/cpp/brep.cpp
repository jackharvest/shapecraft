// Exact boundary representations to and from OCCT shapes. The stream layout is documented in
// crates/kernel/src/brep.rs (`flat::encode`). Every entry point is wrapped like the rest of the shim.

#include <cmath>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <GeomConvert_ApproxSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Precision.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Lin.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <algorithm>
#include <ShapeFix_Shape.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom2dAPI_ProjectPointOnCurve.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_HArray1OfPnt2d.hxx>
#include <Standard_Failure.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_Array2OfReal.hxx>
#include <TColStd_HArray1OfBoolean.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColgp_Array1OfPnt2d.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>

struct SkShape {
  TopoDS_Shape shape;
};

// From shim.cpp.
int sk_fail_external(const char* msg);
void sk_quiet_external();

namespace {

struct Bad {
  std::string why;
};

// A surface whose normal carries on across each knot of full multiplicity (a sweep carried on by a straight piece
// has one: the same direction on both sides, at a different speed, so OCCT calls it C0). Only where the other
// direction is one Bézier piece (a swept arc), so the proof is exact: along the knot line the numerator of each
// side's rational derivative is a polynomial of twice that degree, and the two sides' Bernstein coefficients must
// agree up to one positive factor, which holds along the whole line and not just where sampled.
static bool tangent_joins(const Handle(Geom_BSplineSurface)& s) {
  if (s->IsUPeriodic() || s->IsVPeriodic()) return false;
  for (int dir = 0; dir < 2; ++dir) {
    const bool v = dir == 1;
    const int deg = v ? s->VDegree() : s->UDegree();
    const int nk = v ? s->NbVKnots() : s->NbUKnots();
    const int np = v ? s->NbVPoles() : s->NbUPoles();
    const int q = v ? s->UDegree() : s->VDegree();
    const int rows = v ? s->NbUPoles() : s->NbVPoles();
    auto mult = [&](int k) { return v ? s->VMultiplicity(k) : s->UMultiplicity(k); };
    auto knot = [&](int k) { return v ? s->VKnot(k) : s->UKnot(k); };
    if (mult(1) != deg + 1 || mult(nk) != deg + 1) return false;
    int at = 1;
    for (int k = 2; k < nk; ++k) {
      at += mult(k);
      if (mult(k) < deg) continue;
      if (mult(k) != deg || at <= 1 || at >= np || rows != q + 1 || q > 25) return false;
      const double dl = knot(k) - knot(k - 1), dr = knot(k + 1) - knot(k);
      if (!(dl > 0 && dr > 0)) return false;
      // Relative to a pole on the knot line, so a model far from the origin is judged the same.
      const gp_Pnt anchor = v ? s->Pole(1, at) : s->Pole(at, 1);
      auto h = [&](int row, int i, int c) {
        const gp_Pnt p = v ? s->Pole(row, i) : s->Pole(i, row);
        const double w = v ? s->Weight(row, i) : s->Weight(i, row);
        return c == 3 ? w : (p.Coord(c + 1) - anchor.Coord(c + 1)) * w;
      };
      const size_t n = size_t(rows);
      std::vector<double> on(n * 4), left(n * 4), right(n * 4);
      for (size_t row = 0; row < n; ++row)
        for (size_t c = 0; c < 4; ++c) {
          const int r1 = int(row) + 1, ci = int(c);
          if (!(h(r1, at, 3) > 0 && h(r1, at - 1, 3) > 0 && h(r1, at + 1, 3) > 0)) return false;
          on[row * 4 + c] = h(r1, at, ci);
          left[row * 4 + c] = (h(r1, at, ci) - h(r1, at - 1, ci)) / dl;
          right[row * 4 + c] = (h(r1, at + 1, ci) - h(r1, at, ci)) / dr;
        }
      std::vector<double> choose(size_t(2 * q + 1) * size_t(2 * q + 1), 0.0);
      auto bin = [&](int a, int b) -> double& { return choose[size_t(a) * size_t(2 * q + 1) + size_t(b)]; };
      for (int a = 0; a <= 2 * q; ++a) {
        bin(a, 0) = 1;
        for (int b = 1; b <= a; ++b) bin(a, b) = bin(a - 1, b - 1) + (b < a ? bin(a - 1, b) : 0.0);
      }
      // Bernstein coefficients (degree 2q) of `side_xyz * w - side_w * xyz` along the knot line.
      auto numerator = [&](const std::vector<double>& side) {
        std::vector<double> out(size_t(2 * q + 1) * 3, 0.0);
        for (int i = 0; i <= q; ++i)
          for (int j = 0; j <= q; ++j) {
            const double f = bin(q, i) * bin(q, j) / bin(2 * q, i + j);
            for (size_t c = 0; c < 3; ++c)
              out[size_t(i + j) * 3 + c] += f * (side[size_t(i) * 4 + c] * on[size_t(j) * 4 + 3] - side[size_t(i) * 4 + 3] * on[size_t(j) * 4 + c]);
          }
        return out;
      };
      const std::vector<double> nl = numerator(left), nr = numerator(right);
      double lr = 0, rr = 0, scale = 0;
      for (size_t i = 0; i < nl.size(); ++i) {
        if (!std::isfinite(nl[i]) || !std::isfinite(nr[i])) return false;
        lr += nl[i] * nr[i];
        rr += nr[i] * nr[i];
        scale = std::max(scale, std::abs(nl[i]));
      }
      const double factor = lr / rr;
      if (!(rr > 0 && scale > 0) || !std::isfinite(factor) || !(factor > 0)) return false;
      for (size_t i = 0; i < nl.size(); ++i)
        if (!(std::abs(nl[i] - factor * nr[i]) <= 1e-9 * scale)) return false;
    }
  }
  return true;
}

// Prove C1 across each V join for a ruled rational surface. In homogeneous
// coordinates the numerator of the difference of the two rational V
// derivatives is quadratic in U. Its three Bernstein coefficients bound the
// difference over the entire U interval, not just a set of sampled normals.
static bool ruled_c1_joins(const Handle(Geom_BSplineSurface)& s) {
  if (s->IsUPeriodic() || s->IsVPeriodic() || s->UDegree() != 1 ||
      s->NbUPoles() != 2 || s->NbUKnots() != 2 || s->VDegree() < 2 ||
      s->VMultiplicity(1) != s->VDegree() + 1 ||
      s->VMultiplicity(s->NbVKnots()) != s->VDegree() + 1) return false;
  struct H { double x[4]; };
  auto pole = [&](int u, int v, const gp_Pnt& anchor) {
    H h{}; const gp_Pnt p = s->Pole(u, v); const double w = s->Weight(u, v);
    h.x[0] = (p.X()-anchor.X())*w; h.x[1] = (p.Y()-anchor.Y())*w;
    h.x[2] = (p.Z()-anchor.Z())*w; h.x[3] = w;
    return h;
  };
  int shared = 1;
  for (int k = 2; k < s->NbVKnots(); ++k) {
    const int mult = s->VMultiplicity(k); shared += mult;
    if (mult < s->VDegree()) continue;
    if (mult != s->VDegree() || shared <= 1 || shared >= s->NbVPoles()) return false;
    const double dl = s->VKnot(k)-s->VKnot(k-1), dr = s->VKnot(k+1)-s->VKnot(k);
    if (!(dl > 0 && dr > 0)) return false;
    // Subtract a common local anchor before weighting. The proof must not
    // become more permissive merely because a model is far from the origin.
    const gp_Pnt anchor = s->Pole(1, shared);
    H h[2], d[2], left[2], right[2];
    for (int u = 0; u < 2; ++u) {
      h[u] = pole(u+1, shared, anchor); H a = pole(u+1, shared-1, anchor), b = pole(u+1, shared+1, anchor);
      if (!(h[u].x[3] > 0 && a.x[3] > 0 && b.x[3] > 0)) return false;
      for (int c = 0; c < 4; ++c) {
        if (!std::isfinite(h[u].x[c]) || !std::isfinite(a.x[c]) || !std::isfinite(b.x[c])) return false;
        left[u].x[c] = (h[u].x[c]-a.x[c])/dl;
        right[u].x[c] = (b.x[c]-h[u].x[c])/dr;
        d[u].x[c] = left[u].x[c]-right[u].x[c];
        if (!std::isfinite(left[u].x[c]) || !std::isfinite(right[u].x[c]) || !std::isfinite(d[u].x[c])) return false;
      }
    }
    for (int c = 0; c < 3; ++c) {
      auto cross = [&](const H& a, const H& b) { return a.x[c]*b.x[3]-a.x[3]*b.x[c]; };
      const double q[3] = {cross(d[0],h[0]), (cross(d[0],h[1])+cross(d[1],h[0]))/2, cross(d[1],h[1])};
      double scale = 0;
      for (int a = 0; a < 2; ++a) for (int b = 0; b < 2; ++b) {
        const double lscale = std::abs(left[a].x[c]*h[b].x[3])+std::abs(left[a].x[3]*h[b].x[c]);
        const double rscale = std::abs(right[a].x[c]*h[b].x[3])+std::abs(right[a].x[3]*h[b].x[c]);
        if (!std::isfinite(lscale) || !std::isfinite(rscale)) return false;
        scale = std::max(scale, std::max(lscale, rscale));
      }
      if (!std::isfinite(scale)) return false;
      for (double value : q) if (!std::isfinite(value) || std::abs(value) > 1e-11*std::max(scale, 1e-12)) return false;
    }
  }
  return true;
}

struct Rd {
  const double* d;
  size_t n, at = 0;
  double f() {
    if (at >= n) throw Bad{"brep stream ended early"};
    return d[at++];
  }
  size_t idx(size_t cap) {
    double v = f();
    if (!std::isfinite(v) || v < 0 || v >= double(cap)) throw Bad{"brep index out of range"};
    return size_t(v);
  }
  long opt() {
    double v = f();
    return (std::isfinite(v) && v >= 0) ? long(v) : -1;
  }
  bool b() { return f() != 0.0; }
  gp_Pnt p() {
    double x = f(), y = f(), z = f();
    if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(z)) throw Bad{"non-finite coordinate"};
    return gp_Pnt(x, y, z);
  }
  gp_Dir dir() {
    gp_Pnt v = p();
    gp_Vec w(v.XYZ());
    if (w.Magnitude() < 1e-300) throw Bad{"zero direction"};
    return gp_Dir(w);
  }
};

const size_t CAP = size_t(1) << 24;

gp_Ax3 frame(const gp_Pnt& o, const gp_Dir& z, const gp_Dir& x) {
  // Re-orthogonalize x against z (files are not always exact).
  gp_Vec xv(x.XYZ());
  gp_Vec zv(z.XYZ());
  xv = xv - zv * xv.Dot(zv);
  if (xv.Magnitude() < 1e-12) return gp_Ax3(o, z);
  return gp_Ax3(o, z, gp_Dir(xv));
}

void knots(Rd& r, std::vector<double>& k, std::vector<int>& m) {
  size_t n = r.idx(CAP);
  k.resize(n);
  m.resize(n);
  for (size_t i = 0; i < n; ++i) k[i] = r.f();
  for (size_t i = 0; i < n; ++i) m[i] = int(r.idx(1 << 16));
  // Parasolid keeps knots that differ only in the last bits (seen in real Shapr3D projects); OCCT refuses knots
  // closer than its epsilon. Space them apart by a hair: merging them could push a multiplicity past the degree.
  for (size_t i = 1; i < n; ++i) {
    double gap = 1e-12 * (1.0 + std::fabs(k[i - 1]));
    if (k[i] - k[i - 1] < gap) k[i] = k[i - 1] + gap;
  }
}

// Whether the poles fit a clamped (open) knot vector. Parasolid flags some closed splines periodic while storing
// them clamped, which OCCT's periodic form refuses ("poles and degree mismatch"); read those as open.
bool clamped_fit(int deg, size_t np, const std::vector<int>& m) {
  long sum = 0;
  for (int x : m) sum += x;
  return sum - deg - 1 == long(np);
}

// The parameter of the point of `g` nearest `at` and its distance: an orthogonal projection, or one of the ends of a
// bounded curve when that is nearer (a vertex a little past the end of a spline has no orthogonal foot).
bool nearest_on_curve(const Handle(Geom_Curve)& g, const gp_Pnt& at, double& param, double& dist) {
  bool found = false;
  GeomAPI_ProjectPointOnCurve pr(at, g);
  if (pr.NbPoints() > 0) {
    param = pr.LowerDistanceParameter();
    dist = pr.LowerDistance();
    found = true;
  }
  double ends[2] = {g->FirstParameter(), g->LastParameter()};
  for (double t : ends) {
    if (Precision::IsInfinite(t)) continue;
    double d = g->Value(t).Distance(at);
    if (!found || d < dist) {
      param = t;
      dist = d;
      found = true;
    }
  }
  return found;
}

// The same for a curve in a surface's (u, v): searched along the curve in space, so a periodic surface's seam or a
// vertex just past the curve's end doesn't matter. Samples, then narrows the best bracket.
bool nearest_on_pcurve(const Handle(Geom2d_Curve)& g2, const Handle(Geom_Surface)& s, const gp_Pnt& at, double& param, double& dist) {
  double a = g2->FirstParameter(), b = g2->LastParameter();
  if (Precision::IsInfinite(a) || Precision::IsInfinite(b) || !(b > a)) return false;
  auto d = [&](double t) {
    gp_Pnt2d q = g2->Value(t);
    return s->Value(q.X(), q.Y()).Distance(at);
  };
  const int n = 96;
  int best = 0;
  double bd = d(a);
  for (int k = 1; k <= n; ++k) {
    double v = d(a + (b - a) * k / n);
    if (v < bd) {
      bd = v;
      best = k;
    }
  }
  double lo = a + (b - a) * std::max(best - 1, 0) / n, hi = a + (b - a) * std::min(best + 1, n) / n;
  const double g = 0.5 * (std::sqrt(5.0) - 1.0);
  double x1 = hi - g * (hi - lo), x2 = lo + g * (hi - lo), f1 = d(x1), f2 = d(x2);
  for (int it = 0; it < 80 && hi - lo > 1e-14 * (1.0 + std::fabs(hi)); ++it) {
    if (f1 < f2) {
      hi = x2;
      x2 = x1;
      f2 = f1;
      x1 = hi - g * (hi - lo);
      f1 = d(x1);
    } else {
      lo = x1;
      x1 = x2;
      f1 = f2;
      x2 = lo + g * (hi - lo);
      f2 = d(x2);
    }
  }
  param = 0.5 * (lo + hi);
  dist = d(param);
  if (bd < dist) {
    param = a + (b - a) * best / n;
    dist = bd;
  }
  return std::isfinite(dist);
}

// Puts `g2` (a curve in the face's (u, v) through the edge's vertices, either way round) on the edge for this face,
// cut to the edge's vertices and reparametrized over the edge's range. Leaves the edge alone when the curve doesn't
// follow it; the kernel then projects the edge as before.
void attach_pcurve(BRep_Builder& bb, const TopoDS_Edge& edge, const TopoDS_Face& face, const Handle(Geom_Surface)& s,
                   const Handle(Geom2d_Curve)& g2) {
  try {
    Handle(Geom2d_BSplineCurve) bs = Handle(Geom2d_BSplineCurve)::DownCast(g2);
    if (bs.IsNull()) return;
    double f, l;
    Handle(Geom_Curve) c3 = BRep_Tool::Curve(edge, f, l);
    if (c3.IsNull() || !(l > f)) return;
    TopoDS_Vertex va, vb;
    TopExp::Vertices(edge, va, vb);
    if (va.IsNull() || vb.IsNull()) return;
    double tol = std::max(BRep_Tool::Tolerance(edge), 1e-7);
    double qa, qb, da, db;
    if (va.IsSame(vb)) {
      qa = bs->FirstParameter();
      qb = bs->LastParameter();
    } else {
      if (!nearest_on_pcurve(bs, s, BRep_Tool::Pnt(va), qa, da) || !nearest_on_pcurve(bs, s, BRep_Tool::Pnt(vb), qb, db)) return;
      double vtol = std::max(BRep_Tool::Tolerance(va), BRep_Tool::Tolerance(vb));
      if (da > 10 * vtol + 1e-3 || db > 10 * vtol + 1e-3) return;
    }
    if (std::fabs(qb - qa) < 1e-12) return;
    bool rev = qb < qa;
    Handle(Geom2d_BSplineCurve) cut = Handle(Geom2d_BSplineCurve)::DownCast(bs->Copy());
    double lo = std::min(qa, qb), hi = std::max(qa, qb);
    if (lo > cut->FirstParameter() + 1e-12 || hi < cut->LastParameter() - 1e-12) cut->Segment(lo, hi);
    if (rev) cut->Reverse();
    // Map the curve's range linearly onto the edge's.
    TColStd_Array1OfReal k(1, cut->NbKnots());
    cut->Knots(k);
    double k0 = k(k.Lower()), k1 = k(k.Upper());
    if (!(k1 > k0)) return;
    for (int j = k.Lower(); j <= k.Upper(); ++j) k(j) = f + (k(j) - k0) * (l - f) / (k1 - k0);
    // Keep strictly increasing knots after rounding.
    for (int j = k.Lower() + 1; j <= k.Upper(); ++j)
      if (k(j) <= k(j - 1)) return;
    cut->SetKnots(k);
    // It must follow the edge: compare a few points in space.
    double worst = 0;
    for (int j = 1; j < 8; ++j) {
      double t = f + (l - f) * j / 8.0;
      gp_Pnt2d q = cut->Value(t);
      worst = std::max(worst, s->Value(q.X(), q.Y()).Distance(c3->Value(t)));
    }
    double size = c3->Value(f).Distance(c3->Value(0.5 * (f + l))) + c3->Value(l).Distance(c3->Value(0.5 * (f + l)));
    if (worst > std::max(0.05 * size, 100 * tol) + 1e-3) return;
    bb.UpdateEdge(edge, cut, face, tol);
    if (worst > tol) bb.UpdateEdge(edge, worst * 1.01);
    // The parameters agree only roughly: let SameParameter settle them.
    bb.SameParameter(edge, false);
  } catch (const Standard_Failure&) {
  }
}

// Newton steps from `uv` toward the point of the surface nearest `p` (the foot of the perpendicular), kept inside
// the surface's bounds. Returns the distance reached.
double foot(const Handle(Geom_Surface)& s, const gp_Pnt& p, gp_Pnt2d& uv) {
  double u0, u1, v0, v1;
  s->Bounds(u0, u1, v0, v1);
  // A periodic direction isn't clamped, so the curve stays continuous across the seam.
  bool pu = s->IsUPeriodic(), pv = s->IsVPeriodic();
  auto clamp = [](double x, double lo, double hi, bool periodic) {
    if (periodic || Precision::IsInfinite(lo) || Precision::IsInfinite(hi)) return x;
    return std::min(std::max(x, lo), hi);
  };
  double u = uv.X(), v = uv.Y();
  for (int it = 0; it < 30; ++it) {
    gp_Pnt q;
    gp_Vec su, sv;
    s->D1(u, v, q, su, sv);
    gp_Vec d(q, p);
    double a = su.Dot(su), b = su.Dot(sv), c = sv.Dot(sv), r1 = su.Dot(d), r2 = sv.Dot(d);
    double det = a * c - b * b;
    if (!(std::fabs(det) > 1e-300)) break;
    double du = (r1 * c - r2 * b) / det, dv = (a * r2 - b * r1) / det;
    double nu = clamp(u + du, u0, u1, pu), nv = clamp(v + dv, v0, v1, pv);
    bool done = std::fabs(nu - u) < 1e-13 * (1 + std::fabs(u)) && std::fabs(nv - v) < 1e-13 * (1 + std::fabs(v));
    u = nu;
    v = nv;
    if (done) break;
  }
  uv.SetCoord(u, v);
  return s->Value(u, v).Distance(p);
}

// Curves in (u, v) for the edges of curved faces that have none yet: points along each edge put on the surface by
// Newton steps from the previous one, then interpolated at the edge's own parameters. ShapeFix would otherwise
// project them with a full search for most points, which takes seconds per face on big spline surfaces. An edge whose
// points don't land on the surface is left to ShapeFix.
void pcurves_by_marching(BRep_Builder& bb, const TopoDS_Shape& shape) {
  for (TopExp_Explorer fx(shape, TopAbs_FACE); fx.More(); fx.Next()) {
    TopoDS_Face face = TopoDS::Face(fx.Current());
    TopLoc_Location loc;
    Handle(Geom_Surface) s = BRep_Tool::Surface(face, loc);
    // Planes need no curves stored; ShapeFix projects onto them exactly.
    if (s.IsNull() || !loc.IsIdentity() || s->IsKind(STANDARD_TYPE(Geom_Plane))) continue;
    TopTools_IndexedMapOfShape seen, twice;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next())
      if (seen.Contains(ex.Current())) twice.Add(ex.Current());
      else seen.Add(ex.Current());
    for (int k = 1; k <= seen.Extent(); ++k) {
      TopoDS_Edge e = TopoDS::Edge(seen(k));
      if (twice.Contains(e) || BRep_Tool::Degenerated(e)) continue;
      try {
        double a, b;
        if (!BRep_Tool::CurveOnSurface(e, face, a, b).IsNull()) continue;
        double f, l;
        Handle(Geom_Curve) c = BRep_Tool::Curve(e, f, l);
        if (c.IsNull() || !(l > f)) continue;
        TopLoc_Location el;
        BRep_Tool::Curve(e, el, f, l);
        if (!el.IsIdentity()) continue;
        int n = 24;
        Handle(Geom_BSplineCurve) cb = Handle(Geom_BSplineCurve)::DownCast(c);
        if (!cb.IsNull()) n = std::min(4000, std::max(n, 3 * cb->NbKnots()));
        Handle(TColgp_HArray1OfPnt2d) uv = new TColgp_HArray1OfPnt2d(1, n + 1);
        Handle(TColStd_HArray1OfReal) ts = new TColStd_HArray1OfReal(1, n + 1);
        double tol = std::max(BRep_Tool::Tolerance(e), 1e-6), worst = 0;
        gp_Pnt2d at;
        {
          GeomAPI_ProjectPointOnSurf ps(c->Value(f), s);
          if (ps.NbPoints() == 0) continue;
          double u, v;
          ps.LowerDistanceParameters(u, v);
          at.SetCoord(u, v);
        }
        bool ok = true;
        for (int j = 0; j <= n && ok; ++j) {
          double t = f + (l - f) * j / n;
          double d = foot(s, c->Value(t), at);
          worst = std::max(worst, d);
          if (d > 10 * tol + 1e-2) ok = false;
          uv->SetValue(j + 1, at);
          ts->SetValue(j + 1, t);
        }
        if (!ok) continue;
        // Points must be distinct in (u, v) for the interpolation.
        for (int j = 2; j <= n + 1 && ok; ++j)
          if (uv->Value(j).Distance(uv->Value(j - 1)) < 1e-12) ok = false;
        if (!ok) continue;
        Geom2dAPI_Interpolate in(uv, ts, false, 1e-12);
        in.Perform();
        if (!in.IsDone()) continue;
        Handle(Geom2d_BSplineCurve) pc = in.Curve();
        // Check between the samples.
        for (int j = 0; j < n; ++j) {
          double t = f + (l - f) * (j + 0.5) / n;
          gp_Pnt2d q = pc->Value(t);
          worst = std::max(worst, s->Value(q.X(), q.Y()).Distance(c->Value(t)));
        }
        if (worst > 10 * tol + 1e-2) continue;
        bb.UpdateEdge(e, pc, face, std::max(BRep_Tool::Tolerance(e), worst * 1.5));
      } catch (const Standard_Failure&) {
      }
    }
  }
}

Handle(Geom_Curve) read_curve3(Rd& r, size_t kind) {
  switch (kind) {
    case 0: {
      gp_Pnt o = r.p();
      gp_Dir d = r.dir();
      return new Geom_Line(gp_Ax1(o, d));
    }
    case 1: {
      gp_Pnt c = r.p();
      gp_Dir n = r.dir(), x = r.dir();
      double rad = r.f();
      if (!(rad > 0)) throw Bad{"circle radius"};
      return new Geom_Circle(frame(c, n, x).Ax2(), rad);
    }
    case 2: {
      gp_Pnt c = r.p();
      gp_Dir n = r.dir(), x = r.dir();
      double a = r.f(), b = r.f();
      if (!(a > 0 && b > 0)) throw Bad{"ellipse radii"};
      if (a >= b) return new Geom_Ellipse(frame(c, n, x).Ax2(), a, b);
      // OCCT wants major >= minor: turn the frame a quarter.
      gp_Ax3 f = frame(c, n, x);
      return new Geom_Ellipse(gp_Ax2(c, n, f.YDirection()), b, a);
    }
    case 3: {
      int deg = int(r.idx(64));
      size_t np = r.idx(CAP);
      bool periodic = r.b(), rational = r.b();
      TColgp_Array1OfPnt poles(1, int(np));
      for (size_t i = 0; i < np; ++i) poles(int(i) + 1) = r.p();
      TColStd_Array1OfReal w(1, int(np));
      if (rational)
        for (size_t i = 0; i < np; ++i) w(int(i) + 1) = r.f();
      std::vector<double> k;
      std::vector<int> m;
      knots(r, k, m);
      if (k.empty()) throw Bad{"no knots"};
      TColStd_Array1OfReal kk(1, int(k.size()));
      TColStd_Array1OfInteger mm(1, int(m.size()));
      for (size_t i = 0; i < k.size(); ++i) {
        kk(int(i) + 1) = k[i];
        mm(int(i) + 1) = m[i];
      }
      if (periodic && clamped_fit(deg, np, m)) periodic = false;
      if (rational) return new Geom_BSplineCurve(poles, w, kk, mm, deg, periodic);
      return new Geom_BSplineCurve(poles, kk, mm, deg, periodic);
    }
    case 4: {
      size_t np = r.idx(CAP);
      if (np < 2) throw Bad{"interpolated curve needs two points"};
      Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, int(np));
      for (size_t i = 0; i < np; ++i) pts->SetValue(int(i) + 1, r.p());
      GeomAPI_Interpolate in(pts, false, 1e-7);
      in.Perform();
      if (!in.IsDone()) throw Bad{"could not interpolate an intersection curve"};
      // Spelled out: GCC won't turn the B-spline handle into the base handle on return.
      return Handle(Geom_Curve)(in.Curve());
    }
  }
  throw Bad{"unknown curve kind"};
}

Handle(Geom2d_Curve) read_curve2(Rd& r) {
  int deg = int(r.idx(64));
  size_t np = r.idx(CAP);
  bool periodic = r.b(), rational = r.b();
  TColgp_Array1OfPnt2d poles(1, int(np));
  for (size_t i = 0; i < np; ++i) {
    double u = r.f(), v = r.f();
    poles(int(i) + 1) = gp_Pnt2d(u, v);
  }
  TColStd_Array1OfReal w(1, int(np));
  if (rational)
    for (size_t i = 0; i < np; ++i) w(int(i) + 1) = r.f();
  std::vector<double> k;
  std::vector<int> m;
  knots(r, k, m);
  if (k.empty()) throw Bad{"no knots"};
  TColStd_Array1OfReal kk(1, int(k.size()));
  TColStd_Array1OfInteger mm(1, int(m.size()));
  for (size_t i = 0; i < k.size(); ++i) {
    kk(int(i) + 1) = k[i];
    mm(int(i) + 1) = m[i];
  }
  if (periodic && clamped_fit(deg, np, m)) periodic = false;
  if (rational) return new Geom2d_BSplineCurve(poles, w, kk, mm, deg, periodic);
  return new Geom2d_BSplineCurve(poles, kk, mm, deg, periodic);
}

struct Curve {
  Handle(Geom_Curve) c3;
  Handle(Geom2d_Curve) c2;
  long surface = -1;
};

TopoDS_Shape build(Rd& r) {
  size_t kind = r.idx(3);
  size_t ns = r.idx(CAP);
  // Surfaces may refer to curves (revolution, extrusion) that come later in the stream: keep their
  // references and resolve after the curves are read.
  struct Pending {
    size_t kind;
    std::vector<double> data;
  };
  std::vector<Handle(Geom_Surface)> surfaces(ns);
  std::vector<bool> unchecked_offsets(ns, false);
  std::vector<std::pair<size_t, std::vector<double>>> later;  // index, (kind, curve, origin, axis…)
  std::vector<std::pair<size_t, std::pair<size_t, double>>> offsets;
  for (size_t i = 0; i < ns; ++i) {
    size_t k = r.idx(9);
    switch (k) {
      case 0: {
        gp_Pnt o = r.p();
        gp_Dir n = r.dir(), x = r.dir();
        surfaces[i] = new Geom_Plane(frame(o, n, x));
        break;
      }
      case 1: {
        gp_Pnt o = r.p();
        gp_Dir a = r.dir(), x = r.dir();
        double rad = r.f();
        if (!(rad > 0)) throw Bad{"cylinder radius"};
        surfaces[i] = new Geom_CylindricalSurface(frame(o, a, x), rad);
        break;
      }
      case 2: {
        gp_Pnt o = r.p();
        gp_Dir a = r.dir(), x = r.dir();
        double rad = r.f(), ang = r.f();
        if (!(rad >= 0) || !(std::fabs(ang) > 1e-12 && std::fabs(ang) < M_PI / 2)) throw Bad{"cone angle"};
        surfaces[i] = new Geom_ConicalSurface(frame(o, a, x), ang, rad);
        break;
      }
      case 3: {
        gp_Pnt c = r.p();
        gp_Dir a = r.dir(), x = r.dir();
        double rad = r.f();
        if (!(rad > 0)) throw Bad{"sphere radius"};
        surfaces[i] = new Geom_SphericalSurface(frame(c, a, x), rad);
        break;
      }
      case 4: {
        gp_Pnt c = r.p();
        gp_Dir a = r.dir(), x = r.dir();
        double R = r.f(), rr = r.f();
        if (!(rr > 0 && R != 0 && (R > 0 || -R < rr))) throw Bad{"torus radii"};
        if (R < 0) {
          // XT's lemon torus has a signed major radius (published format reference, TORUS). Revolving its
          // generating circle preserves the exact same (u,v); taking abs(R) would move the fillet surface.
          gp_Dir y = a.Crossed(x);
          Handle(Geom_Circle) circle = new Geom_Circle(gp_Ax2(c.Translated(gp_Vec(x) * R), y.Reversed(), x), rr);
          surfaces[i] = new Geom_SurfaceOfRevolution(circle, gp_Ax1(c, a));
        } else {
          surfaces[i] = new Geom_ToroidalSurface(frame(c, a, x), R, rr);
        }
        break;
      }
      case 5: {
        int ud = int(r.idx(64)), vd = int(r.idx(64));
        size_t nu = r.idx(CAP), nv = r.idx(CAP);
        bool up = r.b(), vp = r.b(), rational = r.b();
        if (nu < 2 || nv < 2 || nu * nv > CAP) throw Bad{"b-surface size"};
        TColgp_Array2OfPnt poles(1, int(nu), 1, int(nv));
        for (size_t a = 0; a < nu; ++a)
          for (size_t b = 0; b < nv; ++b) poles(int(a) + 1, int(b) + 1) = r.p();
        TColStd_Array2OfReal w(1, int(nu), 1, int(nv));
        if (rational)
          for (size_t a = 0; a < nu; ++a)
            for (size_t b = 0; b < nv; ++b) w(int(a) + 1, int(b) + 1) = r.f();
        std::vector<double> uk, vk;
        std::vector<int> um, vm;
        knots(r, uk, um);
        knots(r, vk, vm);
        if (uk.empty() || vk.empty()) throw Bad{"no knots"};
        TColStd_Array1OfReal UK(1, int(uk.size())), VK(1, int(vk.size()));
        TColStd_Array1OfInteger UM(1, int(um.size())), VM(1, int(vm.size()));
        for (size_t j = 0; j < uk.size(); ++j) {
          UK(int(j) + 1) = uk[j];
          UM(int(j) + 1) = um[j];
        }
        for (size_t j = 0; j < vk.size(); ++j) {
          VK(int(j) + 1) = vk[j];
          VM(int(j) + 1) = vm[j];
        }
        if (up && clamped_fit(ud, size_t(poles.ColLength()), um)) up = false;
        if (vp && clamped_fit(vd, size_t(poles.RowLength()), vm)) vp = false;
        if (rational)
          surfaces[i] = new Geom_BSplineSurface(poles, w, UK, VK, UM, VM, ud, vd, up, vp);
        else
          surfaces[i] = new Geom_BSplineSurface(poles, UK, VK, UM, VM, ud, vd, up, vp);
        break;
      }
      case 6: {
        size_t basis = r.idx(CAP);
        double dist = r.f();
        offsets.push_back({i, {basis, dist}});
        break;
      }
      case 7: {
        std::vector<double> v{7, double(r.idx(CAP))};
        for (int j = 0; j < 6; ++j) v.push_back(r.f());
        later.push_back({i, v});
        break;
      }
      default: {
        std::vector<double> v{8, double(r.idx(CAP))};
        for (int j = 0; j < 3; ++j) v.push_back(r.f());
        later.push_back({i, v});
        break;
      }
    }
  }
  size_t nc = r.idx(CAP);
  std::vector<Curve> curves(nc);
  for (size_t i = 0; i < nc; ++i) {
    size_t k = r.idx(6);
    if (k == 5) {
      curves[i].surface = long(r.idx(ns));
      curves[i].c2 = read_curve2(r);
    } else {
      curves[i].c3 = read_curve3(r, k);
    }
  }
  for (auto& l : later) {
    size_t ci = size_t(l.second[1]);
    if (ci >= nc || curves[ci].c3.IsNull()) throw Bad{"swept surface without a 3D curve"};
    const std::vector<double>& v = l.second;
    if (v[0] == 7) {
      gp_Vec ax(v[5], v[6], v[7]);
      if (ax.Magnitude() < 1e-300) throw Bad{"revolution axis"};
      surfaces[l.first] = new Geom_SurfaceOfRevolution(curves[ci].c3, gp_Ax1(gp_Pnt(v[2], v[3], v[4]), gp_Dir(ax)));
    } else {
      gp_Vec d(v[2], v[3], v[4]);
      if (d.Magnitude() < 1e-300) throw Bad{"extrusion direction"};
      surfaces[l.first] = new Geom_SurfaceOfLinearExtrusion(curves[ci].c3, gp_Dir(d));
    }
  }
  // Offsets may stack; resolve until nothing changes.
  for (size_t pass = 0; pass < offsets.size() + 1; ++pass) {
    bool progress = false;
    for (auto& o : offsets) {
      if (!surfaces[o.first].IsNull()) continue;
      size_t b = o.second.first;
      if (b >= ns || surfaces[b].IsNull()) continue;
      // Some files include offset support surfaces that no edge or face uses. OCCT
      // refuses C0 bases because their normals may be ambiguous. Keep a placeholder
      // for dependency resolution and reject it if actual topology refers to it.
      Handle(Geom_BSplineSurface) spline = Handle(Geom_BSplineSurface)::DownCast(surfaces[b]);
      const bool c0 = !spline.IsNull() && spline->Continuity() == GeomAbs_C0;
      const bool smooth = c0 && (ruled_c1_joins(spline) || tangent_joins(spline));
      bool unchecked = unchecked_offsets[b] || (c0 && !smooth);
      surfaces[o.first] = unchecked ? surfaces[b] : Handle(Geom_Surface)(new Geom_OffsetSurface(surfaces[b], o.second.second, smooth));
      unchecked_offsets[o.first] = unchecked;
      progress = true;
    }
    if (!progress) break;
  }
  for (size_t i = 0; i < ns; ++i)
    if (surfaces[i].IsNull()) throw Bad{"unresolved surface"};

  BRep_Builder bb;
  size_t nv = r.idx(CAP);
  std::vector<TopoDS_Vertex> verts(nv);
  for (size_t i = 0; i < nv; ++i) bb.MakeVertex(verts[i], r.p(), 1e-7);

  size_t ne = r.idx(CAP);
  std::vector<TopoDS_Edge> edges(ne);
  std::vector<long> edge_curve(ne, -1);
  for (size_t i = 0; i < ne; ++i) {
    long ci = r.opt();
    edge_curve[i] = ci;
    bool reversed = r.b();
    long v0 = r.opt(), v1 = r.opt();
    double tol = r.f();
    if (ci < 0 || size_t(ci) >= nc) throw Bad{"edge without a curve"};
    if ((v0 >= 0 && size_t(v0) >= nv) || (v1 >= 0 && size_t(v1) >= nv)) throw Bad{"edge vertex out of range"};
    const Curve& c = curves[size_t(ci)];
    if (!c.c3.IsNull()) {
      Handle(Geom_Curve) g = c.c3;
      if (reversed) g = g->Reversed();
      if (v0 >= 0 && v1 >= 0) {
        BRepBuilderAPI_MakeEdge me(g, verts[size_t(v0)], verts[size_t(v1)]);
        if (me.IsDone()) {
          edges[i] = me.Edge();
        } else {
          // A tolerant edge: its vertices sit a little off the curve's ends. Put them where they land on the curve
          // and widen their tolerance to cover the gap, as Parasolid's tolerant modelling does.
          double p[2];
          const TopoDS_Vertex* vv[2] = {&verts[size_t(v0)], &verts[size_t(v1)]};
          for (int k = 0; k < 2; ++k) {
            gp_Pnt at = BRep_Tool::Pnt(*vv[k]);
            double gap = 0;
            if (!nearest_on_curve(g, at, p[k], gap))
              throw Bad{"could not build an edge on its curve (edge " + std::to_string(i) + ": a vertex doesn't project)"};
          if (gap > BRep_Tool::Tolerance(*vv[k])) bb.UpdateVertex(*vv[k], gap * 1.01 + 1e-7);
          }
          BRepBuilderAPI_MakeEdge me2(g, *vv[0], *vv[1], p[0], p[1]);
          if (!me2.IsDone())
            throw Bad{"could not build an edge on its curve (edge " + std::to_string(i) + ", kind " + std::to_string(int(me2.Error())) +
                      ", params " + std::to_string(p[0]) + ".." + std::to_string(p[1]) + ", curve " + g->DynamicType()->Name() + ")"};
          edges[i] = me2.Edge();
        }
      } else {
        BRepBuilderAPI_MakeEdge me(g);
        if (!me.IsDone()) throw Bad{"could not build a closed edge"};
        edges[i] = me.Edge();
      }
    } else {
      Handle(Geom2d_Curve) g2 = c.c2;
      if (reversed) g2 = g2->Reversed();
      if (unchecked_offsets[size_t(c.surface)]) throw Bad{"edge uses an offset of a C0 surface"};
      const Handle(Geom_Surface)& s = surfaces[size_t(c.surface)];
      BRepBuilderAPI_MakeEdge me = (v0 >= 0 && v1 >= 0) ? BRepBuilderAPI_MakeEdge(g2, s, verts[size_t(v0)], verts[size_t(v1)])
                                                        : BRepBuilderAPI_MakeEdge(g2, s);
      if (me.IsDone()) {
        edges[i] = me.Edge();
      } else if (v0 >= 0 && v1 >= 0) {
        // Tolerant edge on a surface: find each vertex on the curve through the surface's (u, v), then build
        // between those parameters with the vertices' tolerance covering the gap.
        double p[2];
        const TopoDS_Vertex* vv[2] = {&verts[size_t(v0)], &verts[size_t(v1)]};
        for (int k = 0; k < 2; ++k) {
          gp_Pnt at = BRep_Tool::Pnt(*vv[k]);
          double gap = 0;
          if (!nearest_on_pcurve(g2, s, at, p[k], gap))
            throw Bad{std::string("could not build an edge on its surface: a vertex is off the curve (edge ") + std::to_string(i) + " on a " +
                      s->DynamicType()->Name() + ")"};
          if (gap > BRep_Tool::Tolerance(*vv[k])) bb.UpdateVertex(*vv[k], gap * 1.01 + 1e-7);
        }
        BRepBuilderAPI_MakeEdge me2(g2, s, *vv[0], *vv[1], p[0], p[1]);
        if (!me2.IsDone())
          throw Bad{std::string("could not build an edge on its surface (edge ") + std::to_string(i) + " on a " + s->DynamicType()->Name() +
                    ", error " + std::to_string(int(me2.Error())) + ")"};
        edges[i] = me2.Edge();
      } else {
        throw Bad{"could not build an edge on its surface"};
      }
      BRepLib::BuildCurves3d(edges[i]);
    }
    if (tol > 0) bb.UpdateEdge(edges[i], tol);
  }

  size_t nf = r.idx(CAP);
  std::vector<TopoDS_Face> faces(nf);
  std::vector<std::pair<size_t, size_t>> given;  // (edge, its curve in this face's (u, v))
  for (size_t i = 0; i < nf; ++i) {
    size_t si = r.idx(ns);
    bool flip = r.b();
    if (unchecked_offsets[si]) throw Bad{"face uses an offset of a C0 surface"};
    size_t nl = r.idx(CAP);
    if (nl == 0) {
      BRepBuilderAPI_MakeFace mf(surfaces[si], 1e-7);
      if (!mf.IsDone()) throw Bad{"could not bound a closed face"};
      faces[i] = mf.Face();
    } else {
      bb.MakeFace(faces[i], surfaces[si], 1e-7);
      for (size_t l = 0; l < nl; ++l) {
        size_t nfin = r.idx(CAP);
        TopoDS_Wire w;
        bb.MakeWire(w);
        for (size_t k = 0; k < nfin; ++k) {
          size_t e = r.idx(ne);
          bool forward = r.b();
          long pc = r.opt();
          if (pc >= 0 && size_t(pc) < nc) given.push_back({e, size_t(pc)});
          TopoDS_Edge ed = edges[e];
          // MakeEdge may return a reversed edge when the vertex order opposes its curve.
          // Compose the fin and face senses with that orientation instead of replacing it.
          if (forward == flip) ed.Reverse();
          bb.Add(w, ed);
        }
        w.Closed(true);
        bb.Add(faces[i], w);
      }
      // The file's own curves of the edges in this face's (u, v), so the kernel needn't project the edges (slow on
      // spline surfaces). Not for an edge the face uses twice (a seam needs two).
      for (auto& g : given) {
        const Curve& c = curves[g.second];
        if (c.c2.IsNull() || c.surface != long(si) || edge_curve[g.first] == long(g.second)) continue;
        if (std::count_if(given.begin(), given.end(), [&](const std::pair<size_t, size_t>& o) { return o.first == g.first; }) > 1) continue;
        attach_pcurve(bb, edges[g.first], faces[i], surfaces[si], c.c2);
      }
      given.clear();
    }
    if (flip) faces[i].Reverse();
  }

  size_t nsh = r.idx(CAP);
  std::vector<TopoDS_Shell> shells;
  for (size_t i = 0; i < nsh; ++i) {
    size_t n = r.idx(CAP);
    TopoDS_Shell sh;
    bb.MakeShell(sh);
    for (size_t k = 0; k < n; ++k) bb.Add(sh, faces[r.idx(nf)]);
    shells.push_back(sh);
  }
  TopoDS_Shape result;
  if (kind == 0) {
    TopoDS_Solid so;
    bb.MakeSolid(so);
    for (auto& sh : shells) {
      sh.Closed(true);
      bb.Add(so, sh);
    }
    result = so;
  } else {
    TopoDS_Compound co;
    bb.MakeCompound(co);
    if (kind == 2) {
      for (auto& edge : edges) bb.Add(co, edge);
      if (edges.empty()) for (auto& vertex : verts) bb.Add(co, vertex);
    } else {
      for (auto& sh : shells) bb.Add(co, sh);
    }
    result = co;
  }
  pcurves_by_marching(bb, result);
  ShapeFix_Shape fix(result);
  fix.SetPrecision(1e-7);
  fix.SetMaxTolerance(1e-2);
  fix.Perform();
  result = fix.Shape();
  BRepLib::SameParameter(result, 1e-6);
  if (kind == 0) {
    GProp_GProps g;
    BRepGProp::VolumeProperties(result, g);
    if (g.Mass() < 0) result.Reverse();
  }
  return result;
}

// ---- OCCT → stream -------------------------------------------------------------------------------

struct Wr {
  std::vector<double> d;
  void f(double v) { d.push_back(v); }
  void n(size_t v) { d.push_back(double(v)); }
  void b(bool v) { d.push_back(v ? 1.0 : 0.0); }
  void p(const gp_Pnt& v) {
    f(v.X());
    f(v.Y());
    f(v.Z());
  }
  void v(const gp_Dir& v) {
    f(v.X());
    f(v.Y());
    f(v.Z());
  }
};

void write_knots(Wr& w, const TColStd_Array1OfReal& k, const TColStd_Array1OfInteger& m) {
  w.n(size_t(k.Length()));
  for (int i = k.Lower(); i <= k.Upper(); ++i) w.f(k(i));
  for (int i = m.Lower(); i <= m.Upper(); ++i) w.n(size_t(m(i)));
}

void write_bcurve(Wr& w, Handle(Geom_BSplineCurve) c) {
  if (c->IsPeriodic()) {
    c = Handle(Geom_BSplineCurve)::DownCast(c->Copy());
    c->SetNotPeriodic();
  }
  w.n(3);
  w.n(size_t(c->Degree()));
  w.n(size_t(c->NbPoles()));
  w.b(false);
  w.b(c->IsRational());
  for (int i = 1; i <= c->NbPoles(); ++i) w.p(c->Pole(i));
  if (c->IsRational())
    for (int i = 1; i <= c->NbPoles(); ++i) w.f(c->Weight(i));
  write_knots(w, c->Knots(), c->Multiplicities());
}

// An edge's 3D curve, in the edge's forward direction.
void write_curve(Wr& w, const TopoDS_Edge& e) {
  double f, l;
  Handle(Geom_Curve) c = BRep_Tool::Curve(e, f, l);  // with the edge's location applied
  if (c.IsNull()) throw Bad{"edge without a 3D curve"};
  GeomAdaptor_Curve a(c, f, l);
  switch (a.GetType()) {
    case GeomAbs_Line: {
      gp_Lin li = a.Line();
      w.n(0);
      w.p(li.Location());
      w.v(li.Direction());
      return;
    }
    case GeomAbs_Circle: {
      gp_Circ ci = a.Circle();
      w.n(1);
      w.p(ci.Location());
      w.v(ci.Axis().Direction());
      w.v(ci.XAxis().Direction());
      w.f(ci.Radius());
      return;
    }
    case GeomAbs_Ellipse: {
      gp_Elips el = a.Ellipse();
      w.n(2);
      w.p(el.Location());
      w.v(el.Axis().Direction());
      w.v(el.XAxis().Direction());
      w.f(el.MajorRadius());
      w.f(el.MinorRadius());
      return;
    }
    default:
      break;
  }
  Handle(Geom_BSplineCurve) bs;
  try {
    bs = GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(c, f, l));
  } catch (const Standard_Failure&) {
    bs.Nullify();
  }
  if (bs.IsNull()) {
    GeomConvert_ApproxCurve ap(new Geom_TrimmedCurve(c, f, l), 1e-6, GeomAbs_C2, 200, 9);
    if (!ap.HasResult()) throw Bad{"could not convert an edge curve"};
    bs = ap.Curve();
  }
  write_bcurve(w, bs);
}

// A face's surface; returns true when its natural normal is opposite to OCCT's surface normal (left-handed frame).
bool write_surface(Wr& w, const TopoDS_Face& face) {
  Handle(Geom_Surface) s = BRep_Tool::Surface(face);  // located
  if (s.IsNull()) throw Bad{"face without a surface"};
  while (s->IsKind(STANDARD_TYPE(Geom_RectangularTrimmedSurface)))
    s = Handle(Geom_RectangularTrimmedSurface)::DownCast(s)->BasisSurface();
  // Preserve a lemon torus as an exact torus on export, including its parametrization. The generic adaptor
  // recognizes a revolved circle geometrically but can choose a different parameter frame.
  Handle(Geom_SurfaceOfRevolution) rev = Handle(Geom_SurfaceOfRevolution)::DownCast(s);
  if (!rev.IsNull()) {
    Handle(Geom_Circle) circle = Handle(Geom_Circle)::DownCast(rev->BasisCurve());
    if (!circle.IsNull()) {
      gp_Ax1 axis = rev->Axis();
      gp_Dir x = circle->Position().XDirection();
      gp_Vec delta(axis.Location(), circle->Location());
      gp_Pnt center = axis.Location().Translated(gp_Vec(axis.Direction()) * delta.Dot(gp_Vec(axis.Direction())));
      gp_Vec radial(center, circle->Location());
      double major = radial.Dot(gp_Vec(x));
      if (major < 0 && std::abs(x.Dot(axis.Direction())) < 1e-10 &&
          circle->Position().YDirection().Dot(axis.Direction()) > 1 - 1e-10 &&
          (radial - gp_Vec(x) * major).Magnitude() < 1e-7) {
        w.n(4);
        w.p(center);
        w.v(axis.Direction());
        w.v(x);
        w.f(major);
        w.f(circle->Radius());
        return false;
      }
    }
  }
  GeomAdaptor_Surface a(s);
  auto frame_out = [&](const gp_Ax3& ax) {
    w.p(ax.Location());
    w.v(ax.Direction());
    w.v(ax.XDirection());
    return !ax.Direct();
  };
  switch (a.GetType()) {
    case GeomAbs_Plane: {
      w.n(0);
      return frame_out(a.Plane().Position());
    }
    case GeomAbs_Cylinder: {
      gp_Cylinder c = a.Cylinder();
      w.n(1);
      bool lh = frame_out(c.Position());
      w.f(c.Radius());
      return lh;
    }
    case GeomAbs_Cone: {
      gp_Cone c = a.Cone();
      w.n(2);
      bool lh = frame_out(c.Position());
      w.f(c.RefRadius());
      w.f(c.SemiAngle());
      return lh;
    }
    case GeomAbs_Sphere: {
      gp_Sphere c = a.Sphere();
      w.n(3);
      bool lh = frame_out(c.Position());
      w.f(c.Radius());
      return lh;
    }
    case GeomAbs_Torus: {
      gp_Torus c = a.Torus();
      w.n(4);
      bool lh = frame_out(c.Position());
      w.f(c.MajorRadius());
      w.f(c.MinorRadius());
      return lh;
    }
    default:
      break;
  }
  Handle(Geom_BSplineSurface) bs = Handle(Geom_BSplineSurface)::DownCast(s);
  if (bs.IsNull()) {
    double u0, u1, v0, v1;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    Handle(Geom_Surface) t = s;
    if (!Precision::IsInfinite(u0) && !Precision::IsInfinite(v0) && !Precision::IsInfinite(u1) && !Precision::IsInfinite(v1)) {
      double du = (u1 - u0) * 0.01, dv = (v1 - v0) * 0.01;
      double U0, U1, V0, V1;
      s->Bounds(U0, U1, V0, V1);
      try {
        t = new Geom_RectangularTrimmedSurface(s, std::max(u0 - du, U0), std::min(u1 + du, U1), std::max(v0 - dv, V0), std::min(v1 + dv, V1));
      } catch (const Standard_Failure&) {
        t = s;
      }
    }
    try {
      bs = GeomConvert::SurfaceToBSplineSurface(t);
    } catch (const Standard_Failure&) {
      bs.Nullify();
    }
    if (bs.IsNull()) {
      GeomConvert_ApproxSurface ap(t, 1e-6, GeomAbs_C1, GeomAbs_C1, 9, 9, 200, 0);
      if (!ap.HasResult()) throw Bad{"could not convert a face surface"};
      bs = ap.Surface();
    }
  } else {
    bs = Handle(Geom_BSplineSurface)::DownCast(bs->Copy());
  }
  if (bs->IsUPeriodic()) bs->SetUNotPeriodic();
  if (bs->IsVPeriodic()) bs->SetVNotPeriodic();
  w.n(5);
  w.n(size_t(bs->UDegree()));
  w.n(size_t(bs->VDegree()));
  w.n(size_t(bs->NbUPoles()));
  w.n(size_t(bs->NbVPoles()));
  w.b(false);
  w.b(false);
  bool rational = bs->IsURational() || bs->IsVRational();
  w.b(rational);
  for (int i = 1; i <= bs->NbUPoles(); ++i)
    for (int j = 1; j <= bs->NbVPoles(); ++j) w.p(bs->Pole(i, j));
  if (rational)
    for (int i = 1; i <= bs->NbUPoles(); ++i)
      for (int j = 1; j <= bs->NbVPoles(); ++j) w.f(bs->Weight(i, j));
  write_knots(w, bs->UKnots(), bs->UMultiplicities());
  write_knots(w, bs->VKnots(), bs->VMultiplicities());
  return false;
}

std::vector<double> describe(const TopoDS_Shape& shape) {
  TopTools_IndexedMapOfShape faces, edges, verts;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  TopExp::MapShapes(shape, TopAbs_VERTEX, verts);
  Wr w;
  bool solid = false;
  for (TopExp_Explorer ex(shape, TopAbs_SOLID); ex.More(); ex.Next()) solid = true;
  w.n(solid ? 0 : (faces.Extent() > 0 ? 1 : 2));
  std::vector<bool> lefty(size_t(faces.Extent()) + 1, false);
  w.n(size_t(faces.Extent()));
  for (int i = 1; i <= faces.Extent(); ++i) lefty[size_t(i)] = write_surface(w, TopoDS::Face(faces(i)));
  // One curve per non-degenerate edge, in the same order as the edges.
  std::vector<long> curve_of(size_t(edges.Extent()) + 1, -1);
  size_t ncurves = 0;
  for (int i = 1; i <= edges.Extent(); ++i)
    if (!BRep_Tool::Degenerated(TopoDS::Edge(edges(i)))) curve_of[size_t(i)] = long(ncurves++);
  w.n(ncurves);
  for (int i = 1; i <= edges.Extent(); ++i)
    if (curve_of[size_t(i)] >= 0) write_curve(w, TopoDS::Edge(edges(i).Oriented(TopAbs_FORWARD)));
  w.n(size_t(verts.Extent()));
  for (int i = 1; i <= verts.Extent(); ++i) w.p(BRep_Tool::Pnt(TopoDS::Vertex(verts(i))));
  w.n(ncurves);
  std::vector<long> edge_index(size_t(edges.Extent()) + 1, -1);
  for (int i = 1; i <= edges.Extent(); ++i) {
    if (curve_of[size_t(i)] < 0) continue;
    TopoDS_Edge e = TopoDS::Edge(edges(i).Oriented(TopAbs_FORWARD));
    edge_index[size_t(i)] = curve_of[size_t(i)];
    TopoDS_Vertex a = TopExp::FirstVertex(e), b = TopExp::LastVertex(e);
    w.n(size_t(curve_of[size_t(i)]));
    w.b(false);
    w.f(a.IsNull() ? -1.0 : double(verts.FindIndex(a) - 1));
    w.f(b.IsNull() ? -1.0 : double(verts.FindIndex(b) - 1));
    double tol = BRep_Tool::Tolerance(e);
    w.f(tol > 1e-5 ? tol : -1.0);
  }
  // Faces as they sit in the shape (orientation composed), loops through the wire explorer.
  std::vector<std::vector<size_t>> shells;
  TopTools_IndexedMapOfShape seen;
  std::vector<std::vector<double>> face_streams(size_t(faces.Extent()) + 1);
  for (TopExp_Explorer sx(shape, TopAbs_SHELL); sx.More(); sx.Next()) {
    std::vector<size_t> sh;
    for (TopExp_Explorer fx(sx.Current(), TopAbs_FACE); fx.More(); fx.Next()) {
      TopoDS_Face face = TopoDS::Face(fx.Current());
      int fi = faces.FindIndex(face);
      if (fi <= 0 || seen.Contains(face)) continue;
      seen.Add(face);
      sh.push_back(size_t(fi - 1));
      Wr fw;
      fw.n(size_t(fi - 1));
      fw.b((face.Orientation() == TopAbs_REVERSED) != lefty[size_t(fi)]);
      std::vector<std::vector<std::pair<size_t, bool>>> loops;
      for (TopExp_Explorer wx(face, TopAbs_WIRE); wx.More(); wx.Next()) {
        std::vector<std::pair<size_t, bool>> fins;
        for (BRepTools_WireExplorer we(TopoDS::Wire(wx.Current()), face); we.More(); we.Next()) {
          const TopoDS_Edge& e = we.Current();
          int ei = edges.FindIndex(e);
          if (ei <= 0 || edge_index[size_t(ei)] < 0) continue;
          fins.push_back({size_t(edge_index[size_t(ei)]), e.Orientation() == TopAbs_FORWARD});
        }
        if (!fins.empty()) loops.push_back(fins);
      }
      fw.n(loops.size());
      for (auto& l : loops) {
        fw.n(l.size());
        for (auto& fin : l) {
          fw.n(fin.first);
          fw.b(fin.second);
          fw.f(-1.0);
        }
      }
      face_streams[size_t(fi)] = fw.d;
    }
    if (!sh.empty()) shells.push_back(sh);
  }
  w.n(seen.Extent());
  for (auto& sh : shells)
    for (size_t fi : sh) w.d.insert(w.d.end(), face_streams[fi + 1].begin(), face_streams[fi + 1].end());
  // Shells refer to positions in the face list just written.
  w.n(shells.size());
  size_t at = 0;
  for (auto& sh : shells) {
    w.n(sh.size());
    for (size_t k = 0; k < sh.size(); ++k) w.n(at++);
  }
  return w.d;
}

template <class F>
int guard_b(F&& f) {
  try {
    sk_quiet_external();
    return f();
  } catch (const Bad& b) {
    return sk_fail_external(b.why.c_str());
  } catch (const Standard_Failure& e) {
    return sk_fail_external(e.GetMessageString());
  } catch (const std::exception& e) {
    return sk_fail_external(e.what());
  } catch (...) {
    return sk_fail_external("unknown C++ exception");
  }
}

}  // namespace

extern "C" {

int sk_from_brep(const double* data, size_t n, SkShape** out) {
  return guard_b([&] {
    Rd r{data, n};
    TopoDS_Shape s = build(r);
    if (s.IsNull()) return sk_fail_external("empty body");
    *out = new SkShape{s};
    return 0;
  });
}

int sk_to_brep(const SkShape* s, double** out, size_t* n) {
  return guard_b([&] {
    std::vector<double> d = describe(s->shape);
    double* buf = new double[d.size() ? d.size() : 1];
    std::copy(d.begin(), d.end(), buf);
    *out = buf;
    *n = d.size();
    return 0;
  });
}

void sk_doubles_free(double* d) { delete[] d; }

}  // extern "C"
