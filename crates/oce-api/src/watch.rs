//! Stateless, key-selected reads of current output connector values.
//!
//! This module registers no watches, stores no watch state, and has no presence on the tick path.

use oce_model::Value;
use oce_store::Store;

use crate::{Engine, OcError};

impl<S: Store> Engine<S> {
    /// Reads selected output point values in caller order.
    ///
    /// Reads are post-tick: after `tick(t)` returns, this method returns exactly tick `t`'s values.
    /// Torn reads are impossible because `tick` takes `&mut self` and [`Engine`] is not `Clone`, so
    /// there is a single writer.
    ///
    /// Keys are output point paths; every output connector, internal and boundary alike, is
    /// addressable. After CXF import, only boundary inputs and lowered pass-through ports carry
    /// IRIs; essentially every output path is synthetic (`conn#<id>`), so path form does not tell a
    /// host whether a point is internal. Hosts wanting stable labels map
    /// [`crate::TopologyBlock::instance_path`] to that block's ordered
    /// [`crate::TopologyBlock::outputs`] entries and use those connector paths as keys; lowered
    /// pass-through ports are reported in [`crate::Topology::pass_through`] rather than in
    /// `blocks`, so that recipe does not reach them. An instance path itself identifies a block
    /// and is never a valid key.
    ///
    /// Returned pairs echo the supplied keys, including duplicates; an empty `points` slice
    /// returns `Ok` with an empty vector. If duplicate output paths exist in the model, the
    /// first connector wins, matching [`Engine::get_output`]. This deliberately differs from
    /// [`crate::Outputs::to_map`], which returns one row per connector. Callers with owned
    /// `String` paths can create a key slice with
    /// `paths.iter().map(String::as_str).collect::<Vec<_>>()`.
    ///
    /// ```
    /// use oce_api::{Engine, Value};
    ///
    /// let cxf = br##"{
    ///   "@context": {
    ///     "S231": "http://data.ashrae.org/S231P#",
    ///     "base": "http://example.org#"
    ///   },
    ///   "@graph": [
    ///     {
    ///       "@id": "http://example.org#Example",
    ///       "@type": "S231:Block",
    ///       "S231:label": "Example",
    ///       "S231:containsBlock": [
    ///         { "@id": "http://example.org#Example.con" },
    ///         { "@id": "http://example.org#Example.gain" }
    ///       ],
    ///       "S231:hasOutput": { "@id": "http://example.org#Example.y" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.con",
    ///       "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
    ///       "S231:label": "con",
    ///       "S231:hasParameter": { "@id": "http://example.org#Example.con.k" },
    ///       "S231:hasOutput": { "@id": "http://example.org#Example.con.y" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.con.k",
    ///       "S231:value": {
    ///         "@value": "2.0",
    ///         "@type": "http://www.w3.org/2001/XMLSchema#double"
    ///       }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.con.y",
    ///       "@type": "S231:RealOutput",
    ///       "S231:isOfDataType": { "@id": "S231:Real" },
    ///       "S231:isConnectedTo": { "@id": "http://example.org#Example.gain.u" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.gain",
    ///       "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
    ///       "S231:label": "gain",
    ///       "S231:hasParameter": { "@id": "http://example.org#Example.gain.k" },
    ///       "S231:hasInput": { "@id": "http://example.org#Example.gain.u" },
    ///       "S231:hasOutput": { "@id": "http://example.org#Example.gain.y" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.gain.k",
    ///       "S231:value": {
    ///         "@value": "3.0",
    ///         "@type": "http://www.w3.org/2001/XMLSchema#double"
    ///       }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.gain.u",
    ///       "@type": "S231:RealInput",
    ///       "S231:isOfDataType": { "@id": "S231:Real" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.gain.y",
    ///       "@type": "S231:RealOutput",
    ///       "S231:isOfDataType": { "@id": "S231:Real" },
    ///       "S231:isConnectedTo": { "@id": "http://example.org#Example.y" }
    ///     },
    ///     {
    ///       "@id": "http://example.org#Example.y",
    ///       "@type": "S231:RealOutput",
    ///       "S231:isOfDataType": { "@id": "S231:Real" }
    ///     }
    ///   ]
    /// }"##;
    /// let mut engine = Engine::in_memory();
    /// engine.load_cxf(cxf)?;
    /// // The multiply's output has an upstream input, so its slot is 0.0 until a tick runs —
    /// // reading it before and after pins that watch returns exactly tick t's values.
    /// let before = engine.watch(&["conn#2"])?;
    /// assert!(before[0].1.bit_eq(&Value::Real(0.0)));
    /// engine.tick(0.0)?;
    /// let after = engine.watch(&["conn#2"])?;
    /// assert!(after[0].1.bit_eq(&Value::Real(6.0)));
    /// # Ok::<(), oce_api::OcError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`OcError::UnknownPoint`] naming the first unresolvable key in caller order.
    /// This method never panics.
    pub fn watch(&self, points: &[&str]) -> Result<Vec<(String, Value)>, OcError> {
        let connectors = points
            .iter()
            .map(|point| {
                self.io
                    .resolve_output(point)
                    .ok_or_else(|| OcError::UnknownPoint((*point).to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(points
            .iter()
            .zip(connectors)
            .map(|(point, connector)| {
                (
                    (*point).to_owned(),
                    self.state.values[connector.0 as usize].clone(),
                )
            })
            .collect())
    }
}
