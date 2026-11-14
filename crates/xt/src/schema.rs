//! Node layouts. The base is schema 13006 (the one embedded schemas are written against), taken from the
//! Parasolid XT Format Reference. Files from newer modellers patch it per node type (see `read`).

use std::collections::HashMap;

/// One field of a node: a type letter from the reference (`d p f v c l u n w i b h t`) and the element count
/// (0 = scalar, 1 = variable length, n > 1 = fixed array).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub kind: u8,
    pub n: u32,
    /// Pointer class (node type or class) for `p` fields; 0 otherwise.
    pub class: u16,
    /// Taken from the base schema (as opposed to declared by the file).
    pub base: bool,
    /// The transmit flag a file declared for a variable-length field.
    pub xmt: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDef {
    pub ty: u16,
    pub name: String,
    /// Only for node types a file declared in full.
    pub description: String,
    pub fields: Vec<FieldDef>,
}

impl NodeDef {
    pub fn variable(&self) -> bool {
        self.fields.last().is_some_and(|f| f.n == 1)
    }
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.name == name)
    }
}

/// `type NAME: field kind[n]; …` — `p` fields may carry a class after a slash (`owner p/1011`), `[]` = variable.
const BASE: &str = "
10 ASSEMBLY: highest_node_id d; attributes_groups p; attribute_chains p; list p; surface p; curve p; point p; key p; res_size f; res_linear f; ref_instance p; next p; previous p; state u; owner p; type u; sub_instance p
11 INSTANCE: node_id d; attributes_groups p; type u; part p; transform p; assembly p; next_in_part p; prev_in_part p; next_of_part p; prev_of_part p
12 BODY: highest_node_id d; attributes_groups p; attribute_chains p; surface p; curve p; point p; key p; res_size f; res_linear f; ref_instance p; next p; previous p; state u; owner p; body_type u; nom_geom_state u; shell p; boundary_surface p; boundary_curve p; boundary_point p; region p; edge p; vertex p
13 SHELL: node_id d; attributes_groups p; body p; next p; face p; edge p; vertex p; region p; front_face p
14 FACE: node_id d; attributes_groups p; tolerance f; next p; previous p; loop p; shell p; surface p; sense c; next_on_surface p; previous_on_surface p; next_front p; previous_front p; front_shell p
15 LOOP: node_id d; attributes_groups p; fin p; face p; next p
16 EDGE: node_id d; attributes_groups p; tolerance f; fin p; previous p; next p; curve p; next_on_curve p; previous_on_curve p; owner p
17 FIN: attributes_groups p; loop p; forward p; backward p; vertex p; other p; edge p; curve p; next_at_vx p; sense c
18 VERTEX: node_id d; attributes_groups p; fin p; previous p; next p; point p; tolerance f; owner p
19 REGION: node_id d; attributes_groups p; body p; next p; previous p; shell p; type c
29 POINT: node_id d; attributes_groups p; owner p; next p; previous p; pvec v
30 LINE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; pvec v; direction v
31 CIRCLE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; centre v; normal v; x_axis v; radius f
32 ELLIPSE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; centre v; normal v; x_axis v; major_radius f; minor_radius f
38 INTERSECTION: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; surface p[2]; chart p; start p; end p
40 CHART: base_parameter f; base_scale f; chart_count d; chordal_error f; angular_error f; parameter_error f[2]; hvec h[]
41 LIMIT: type c; hvec h[]
45 BSPLINE_VERTICES: vertices f[]
50 PLANE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; pvec v; normal v; x_axis v
51 CYLINDER: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; pvec v; axis v; radius f; x_axis v
52 CONE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; pvec v; axis v; radius f; sin_half_angle f; cos_half_angle f; x_axis v
53 SPHERE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; centre v; radius f; axis v; x_axis v
54 TORUS: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; centre v; axis v; major_radius f; minor_radius f; x_axis v
56 BLENDED_EDGE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; blend_type c; surface p[2]; spine p; range f[2]; thumb_weight f[2]; boundary p[2]; start p; end p
59 BLEND_BOUND: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; boundary n; blend p
60 OFFSET_SURF: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; check c; true_offset l; surface p; offset f; scale f
67 SWEPT_SURF: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; section p; sweep v; scale f
68 SPUN_SURF: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; profile p; base v; axis v; start v; end v; start_param f; end_param f; x_axis v; scale f
70 LIST: node_id d; owner p; next p; previous p; list_type d; list_length d; block_length d; size_of_entry d; list_block p; finger_block p; finger_index d; notransmit l
74 POINTER_LIS_BLOCK: n_entries d; next_block p; entries p[]
79 ATT_DEF_ID: string c[]
80 ATTRIB_DEF: next p; identifier p; type_id d; actions u[8]; field_names p; legal_owners l[14]; fields u[]
81 ATTRIBUTE: node_id d; definition p; owner p; next p; previous p; next_of_type p; previous_of_type p; fields p[]
82 INT_VALUES: values d[]
83 REAL_VALUES: values f[]
84 CHAR_VALUES: values c[]
85 POINT_VALUES: values v[]
86 VECTOR_VALUES: values v[]
87 AXIS_VALUES: values v[]
88 TAG_VALUES: values t[]
89 DIRECTION_VALUES: values v[]
90 GROUP: node_id d; attributes_groups p; owner p; next p; previous p; type u; first_member p
91 MEMBER_OF_GROUP: dummy_node_id d; owning_group p; owner p; next p; previous p; next_member p; previous_member p
98 UNICODE_VALUES: values w[]
99 FIELD_NAMES: names p[]
100 TRANSFORM: node_id d; owner p; next p; previous p; rotation_matrix f[9]; translation_vector v; scale f; flag d; perspective_vector v
101 WORLD: assembly p; attribute p; body p; transform p; surface p; curve p; point p; alive l; attrib_def p; highest_id d; current_id d
102 KEY: string c[]
120 PE_SURF: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; type c; data p; tf p; internal_geom p[]
121 INT_PE_DATA: geom_type d; real_array p; int_array p
122 EXT_PE_DATA: key p; real_array p; int_array p
124 B_SURFACE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; nurbs p; data p
125 SURFACE_DATA: original_uint i; original_vint i; extended_uint i; extended_vint i; self_int u; original_u_start c; original_u_end c; original_v_start c; original_v_end c; extended_u_start c; extended_u_end c; extended_v_start c; extended_v_end c; analytic_form_type c; swept_form_type c; spun_form_type c; blend_form_type c; analytic_form p; swept_form p; spun_form p; blend_form p
126 NURBS_SURF: u_periodic l; v_periodic l; u_degree n; v_degree n; n_u_vertices d; n_v_vertices d; u_knot_type u; v_knot_type u; n_u_knots d; n_v_knots d; rational l; u_closed l; v_closed l; surface_form u; vertex_dim n; bspline_vertices p; u_knot_mult p; v_knot_mult p; u_knots p; v_knots p
127 KNOT_MULT: mult n[]
128 KNOT_SET: knots f[]
130 PE_CURVE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; type c; data p; tf p; internal_geom p[]
133 TRIMMED_CURVE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; basis_curve p; point_1 v; point_2 v; parm_1 f; parm_2 f
134 B_CURVE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; nurbs p; data p
135 CURVE_DATA: self_int u; analytic_form p
136 NURBS_CURVE: degree n; n_vertices d; vertex_dim n; n_knots d; knot_type u; periodic l; closed l; rational l; curve_form u; bspline_vertices p; knot_mult p; knots p
137 SP_CURVE: node_id d; attributes_groups p; owner p; next p; previous p; geometric_owner p; sense c; surface p; b_curve p; original p; tolerance_to_original f
141 GEOMETRIC_OWNER: owner p; next p; previous p; shared_geometry p
163 HELIX_SU_FORM: axis_pt v; axis_dir v; hand c; turns i; pitch f; gap f; tol f
184 HELIX_CU_FORM: axis_pt v; axis_dir v; point v; hand c; turns i; pitch f; tol f
";

/// The base schema (13006), keyed by node type.
pub fn base() -> HashMap<u16, NodeDef> {
    let mut out = HashMap::new();
    for line in BASE.lines() {
        let Some((head, body)) = line.split_once(':') else { continue };
        let mut head = head.split_whitespace();
        let (Some(ty), Some(name)) = (head.next().and_then(|t| t.parse::<u16>().ok()), head.next()) else { continue };
        let fields = body
            .split(';')
            .filter_map(|f| {
                let mut it = f.split_whitespace();
                let name = it.next()?;
                let spec = it.next()?;
                let kind = *spec.as_bytes().first()?;
                let n = match spec.find('[') {
                    None => 0,
                    Some(i) => {
                        let inner = spec.get(i + 1..spec.len().saturating_sub(1)).unwrap_or("");
                        if inner.is_empty() { 1 } else { inner.parse().unwrap_or(0) }
                    }
                };
                Some(FieldDef { name: name.to_string(), kind, n, class: 0, base: true, xmt: true })
            })
            .collect();
        out.insert(ty, NodeDef { ty, name: name.to_string(), description: String::new(), fields });
    }
    out
}

/// Fixed-size base schema field counts are part of the format; keep a test that the table parses fully.
#[cfg(test)]
mod tests {
    #[test]
    fn base_table_parses() {
        let b = super::base();
        assert_eq!(b.len(), 65);
        assert_eq!(b[&12].fields.len(), 23);
        assert_eq!(b[&10].fields.len(), 17);
        assert!(b[&84].variable());
        assert_eq!(b[&100].fields[4].n, 9);
    }
}
