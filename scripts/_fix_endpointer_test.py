from pathlib import Path

path = Path("crates/blcvoice-endpointer/src/lib.rs")
source = path.read_text()
source = source.replace(
    '        endpointer.observe(observation(-42.0, 100)).unwrap();\n        let snapshot = endpointer.snapshot();\n        assert_eq!(snapshot.noise_floor_dbfs, -42.0);\n        assert_eq!(snapshot.start_threshold_dbfs, -30.0);',
    '        endpointer.observe(observation(-50.0, 100)).unwrap();\n        let snapshot = endpointer.snapshot();\n        assert_eq!(snapshot.noise_floor_dbfs, -50.0);\n        assert_eq!(snapshot.start_threshold_dbfs, -38.0);',
    1,
)
path.write_text(source)
