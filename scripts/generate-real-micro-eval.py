"""Build deterministic, synthetic RN-shaped fixtures; contains no user data."""
import json
from pathlib import Path

rows = []


def add(task, input_, expected):
    rows.append({"id": f"real-micro-{len(rows) + 1:03}", "task": task,
                 "input": input_, "expected": expected})


labels = ["biology", "physics", "chemistry", "computing", "mathematics", "unknown"]
queries = [
    ("Find randomized trials of aerobic exercise and systolic pressure in adults with hypertension.", ["randomized trials", "aerobic exercise", "systolic pressure", "hypertension"]),
    ("Compare lithium iron phosphate with nickel manganese cobalt cells after 500 cycles.", ["lithium iron phosphate", "nickel manganese cobalt", "500 cycles"]),
    ("Measure CRISPR-Cas9 off-target edits in primary human T cells.", ["CRISPR-Cas9", "off-target", "T cells"]),
    ("Find meta-analyses of sleep duration and insulin resistance in adolescents.", ["meta-analyses", "sleep duration", "insulin resistance", "adolescents"]),
    ("Field studies of drought stress and maize yield under rain-fed conditions.", ["drought stress", "maize yield", "rain-fed"]),
    ("Replication studies of the Stroop effect in bilingual adults.", ["replication", "Stroop effect", "bilingual adults"]),
    ("Find confidence intervals for PM2.5 and asthma admissions.", ["confidence intervals", "PM2.5", "asthma admissions"]),
    ("Studies reporting that intervention B did not significantly improve six-month retention.", ["did not significantly improve", "six-month retention"]),
    ("Whether metformin changes all-cause mortality in older adults.", ["metformin", "all-cause mortality", "older adults"]),
    ("Dose-response curves for caffeine and reaction time, including null results.", ["dose-response", "caffeine", "reaction time", "null results"]),
    ("Pre-registered 2×2 factorial experiments on soil nitrogen and irrigation.", ["pre-registered", "2×2 factorial", "soil nitrogen", "irrigation"]),
    ("Search bilingual sources for фотосинтез under low-light conditions.", ["фотосинтез", "low-light"]),
    ("ΔG° and pH-dependent binding affinity for enzyme E7.", ["ΔG°", "pH-dependent", "binding affinity", "E7"]),
    ("Compare urban and rural vaccine adverse-event rates, preserving cohort setting.", ["urban", "rural", "vaccine", "adverse-event rates"]),
    ("Evidence distinguishing correlation from causation in microplastic exposure.", ["correlation", "causation", "microplastic exposure"]),
    ("Preliminary estimates for dataset Kappa, n=84.", ["Preliminary", "Kappa", "n=84"]),
    ("Separate null and significant findings for compound R.", ["null", "significant", "compound R"]),
    ("Model A versus model B on held-out data, not training accuracy.", ["model A", "model B", "held-out", "not training accuracy"]),
    ("Median and interquartile range of post-surgery recovery time.", ["median", "interquartile range", "recovery time"]),
    ("Systematic review of evidence quality, not efficacy, for supplement Z.", ["systematic review", "evidence quality", "supplement Z"]),
    ("Experiments with n≤30 and reported effect size d=0.4.", ["n≤30", "d=0.4", "effect size"]),
    ("Follow-up longer than 12 months after intervention B.", ["follow-up", "12 months", "intervention B"]),
    ("Conflicting conductivity measurements at 25 °C in polymer P.", ["conflicting", "conductivity", "25 °C", "polymer P"]),
    ("Explicitly excluded participants and reported exclusion reasons.", ["excluded participants", "exclusion reasons"]),
    ("Gene expression in cell line X under hypoxia; do not broaden the search.", ["gene expression", "cell line X", "hypoxia"]),
]
for query, terms in queries:
    add("query_rewrite", {"query": query, "mandatory_terms": []}, {"intent_terms": terms})

quotes = [
    ["Trial A included 42 participants.", "Median follow-up was 8 weeks."],
    ["The treatment did not reduce mortality (RR 0.98, 95% CI 0.81–1.19)."],
    ["Preliminary data suggest a possible increase in yield; the authors call the result uncertain."],
    ["Group A improved by 4 points.", "Group B declined by 2 points."],
    ["No statistically significant difference was observed between arms."],
    ["The source reports 12 of 30 samples were positive, but does not identify the assay."],
    ["According to the authors, the association may be confounded by age."],
    ["Dataset Ω showed ΔG° = −12.4 kJ/mol at pH 7.0."],
    ["У групі B не виявили статистично значущої різниці (p=0,08)."],
    ["No adverse events were recorded.", "It does not state whether monitoring was complete."],
    ["The paper reports a 3% increase in one bench test."],
    ["The sample contained 5 μg/mL of compound C; concentration after 24 h was below detection."],
    ["Results were consistent with benefit, although the 95% CI crossed zero."],
    ["Study 1 found higher uptake.", "Study 2 found no change in uptake."],
    ["Participants: n=18; two withdrew before the endpoint was measured."],
    ["The authors did not measure toxicity and make no safety claim."],
    ["Treatment reduced symptoms in 7/20 cases (35%); the uncontrolled design limits attribution."],
    ["The measured effect was 0.1 SD (CI −0.2 to 0.4)."],
    ["α=0.05 was specified before analysis; the observed p-value was 0.051."],
    ["In vitro only: the compound inhibited enzyme E3 by 22% at 10 μM."],
    [], [], [], [],
    ["Yield was 11 kg/ha."],
    ["Source X reports 2.1±0.3; source Y reports 1.4±0.2 under a different protocol."],
    ["Not all participants completed follow-up; denominator at month 6 is unavailable."],
    ["At 25 °C, resistance was 8.2 Ω.", "At 40 °C, resistance was 9.1 Ω."],
    ["Correction: the earlier value 14.2 was a transcription error; the corrected value is 12.4."],
    ["The authors state that causality cannot be inferred from this cross-sectional association."],
]
texts = [
    "Trial K randomized 84 adults. At 12 weeks, mean systolic pressure fell 3 mmHg in the intervention group; the confidence interval included zero.",
    "Battery B completed 500 cycles. Capacity retention was 91% at 25 °C; testing used three cells.",
    "The assay detected E7 in 18 of 24 samples. Two failed quality control; authors marked results preliminary.",
    "In a cross-sectional survey of 312 students, sleep under six hours correlated with fatigue; causality was not tested.",
    "The field study measured maize yield on 16 rain-fed plots during one drought season. Median yield was 4.2 t/ha.",
    "Model A achieved 0.81 and Model B 0.79 accuracy on held-out data (n=900). Differences were not statistically tested.",
    "At 25 °C, polymer P conductivity was 1.8 mS/cm (n=5); another protocol measured 1.2 mS/cm.",
    "Compound R reduced symptom score by 2 points in 7 of 20 uncontrolled cases. No comparison group was used.",
    "The pre-registered experiment included 40 people; 36 completed follow-up at 6 months. Attrition reasons were not reported.",
    "Dataset Kappa contains 84 rows. Missing values used median substitution; results are preliminary.",
    "CRISPR-Cas9 edits appeared at two candidate off-target sites in vitro. No in-vivo assay was performed.",
    "The review included 11 studies but rated certainty low because of imprecision and inconsistency.",
    "Median recovery was 9 days (IQR 6–14) among 52 participants; one was lost to follow-up.",
    "Estimated effect d=0.4, 95% CI −0.1 to 0.9, n=28. The interval includes no effect.",
    "Study A reported 12% uptake; Study B reported 9% with a different instrument. Results are not comparable.",
    "The paper reports no significant mortality difference (RR 0.98, 95% CI 0.81–1.19) after 2 years.",
    "Cultures received 5 μM for 24 h. Viability fell 8%; biological replicates n=3.",
    "Urban cohort n=120 and rural cohort n=98 had different adverse-event rates; ascertainment differed by site.",
    "Authors observed a possible yield increase but state the sample is underpowered and estimate uncertain.",
    "At pH 7.0, ΔG° was −12.4 kJ/mol; at pH 6.0 it was −10.1 kJ/mol. Both are model-derived.",
]
facts = [
    ["84 adults", "12 weeks", "3 mmHg", "included zero"], ["500 cycles", "91%", "25 °C", "three cells"],
    ["18 of 24", "Two failed quality control", "preliminary"], ["312 students", "under six hours", "correlated", "causality was not tested"],
    ["16 rain-fed plots", "one drought season", "4.2 t/ha"], ["0.81", "0.79", "held-out data", "not statistically tested"],
    ["25 °C", "1.8 mS/cm", "n=5", "1.2 mS/cm"], ["2 points", "7 of 20", "No comparison group"],
    ["40 people", "36 completed", "6 months", "not reported"], ["84 rows", "median substitution", "preliminary"],
    ["two candidate off-target sites", "in vitro", "No in-vivo assay"], ["11 studies", "certainty low", "imprecision", "inconsistency"],
    ["9 days", "IQR 6–14", "52 participants", "lost to follow-up"], ["d=0.4", "95% CI −0.1 to 0.9", "n=28", "includes no effect"],
    ["12%", "9%", "different instrument", "not comparable"], ["no significant mortality difference", "RR 0.98", "2 years"],
    ["5 μM", "24 h", "8%", "n=3"], ["n=120", "n=98", "ascertainment differed"],
    ["possible yield increase", "underpowered", "uncertain"], ["pH 7.0", "−12.4 kJ/mol", "pH 6.0", "model-derived"],
]
for index, text in enumerate(texts):
    add("short_summary", {"text": text, "mandatory_facts": facts[index], "maximum_characters": 320}, {"required_spans": facts[index]})
for index, text in enumerate(texts):
    refs = [f"SRC-{index + 1:02}", f"W-{index + 1:02}"]
    full = f"Process note {refs[1]} cites {refs[0]}. {text} Repeated editorial commentary can be removed."
    mandatory = refs[:1] + facts[index]
    add("context_compression", {"text": full, "mandatory_facts": mandatory, "maximum_characters": 360}, {"required_spans": mandatory})

tag_cases = [
    ("Mitosis in cultured root cells.", ["Biology"]), ("Catalyst accelerated iron oxidation.", ["Chemistry"]),
    ("Photon energy from measured wavelength.", ["Physics"]), ("Compiler optimized a sorting routine.", ["Computing"]),
    ("Proof bounds prime gaps by N.", ["Mathematics"]), ("Gene expression parsed with a sequencing pipeline.", ["Biology", "Computing"]),
    ("Reaction enthalpy estimated by numerical integration.", ["Chemistry", "Mathematics"]), ("Cell motility tracked by microscopy.", ["Biology"]),
    ("Electric field accelerated ions through solution.", ["Physics", "Chemistry"]), ("Bayesian model estimates allele frequencies.", ["Biology", "Mathematics"]),
    ("Meeting reminder: no scientific subject is discussed.", []), ("ΔG° and pH describe the binding assay.", ["Chemistry"]),
    ("Algorithm calculates a confidence interval.", ["Computing", "Mathematics"]), ("Light varied across plant growth chambers.", ["Physics", "Biology"]),
    ("Rainfall observations stored in SQL.", ["Computing"]), ("Виміряли фотосинтез за низького освітлення.", ["Biology"]),
    ("Protein model predicts a β-sheet structure.", ["Biology"]), ("Temperature rose 4 °C during the reaction.", ["Physics", "Chemistry"]),
    ("If n is prime, it has exactly two positive divisors.", ["Mathematics"]), ("Cell counts summarized with Python.", ["Biology", "Computing"]),
]
for text, expected in tag_cases:
    add("tagging", {"text": text, "allowed_tags": ["Biology", "Chemistry", "Physics", "Computing", "Mathematics"]}, {"tags": expected})

class_cases = [
    ("Mitosis in leaf tissue.", "biology"), ("Photon wavelength was 532 nm.", "physics"),
    ("Sodium chloride dissolved in water.", "chemistry"), ("Compiler emitted an optimization warning.", "computing"),
    ("Lemma bounds a sequence for prime n.", "mathematics"), ("Gene-expression matrix parsed by Python.", "biology"),
    ("Catalyst changed reaction enthalpy by 8 kJ/mol.", "chemistry"), ("Electric field accelerated a charged particle.", "physics"),
    ("Recursive algorithm has O(n log n) complexity.", "computing"), ("Proof uses induction over integer k.", "mathematics"),
    ("No topic: this paragraph lists meeting times.", "unknown"), ("β-galactosidase in bacterial cells.", "biology"),
    ("Solution had pH 3.2 and formed a precipitate.", "chemistry"), ("Detector recorded 2.4 GHz oscillations.", "physics"),
    ("SQL query plans compared two indexes.", "computing"), ("Виміряли швидкість фотосинтезу клітин.", "biology"),
    ("Dataset median computed by a statistical formula.", "mathematics"), ("Copper oxidation tracked by spectroscopy.", "chemistry"),
    ("Lens focused ultraviolet radiation on a sensor.", "physics"), ("Code stores allele counts in a dictionary.", "computing"),
    ("Cell growth depends on photon flux.", "biology"), ("Matrix eigenvalue was 2.7.", "mathematics"),
    ("Polymer sample melted at 164 °C.", "chemistry"), ("Magnetic field changed electron trajectories.", "physics"),
    ("Algorithm identifies duplicated gene sequences.", "computing"),
]
for text, expected in class_cases:
    add("classification", {"text": text, "labels": labels}, {"label": expected})

for index, statements in enumerate(quotes):
    text = " ".join(statements)
    if index == 20:
        text = ""
    elif index == 21:
        text = "   "
    elif index == 22:
        text = "Appendix label: Figure 4. No result is stated here."
    elif index == 23:
        text = "!!! ???"
    elif index == 24:
        text = "The abstract repeats the same sentence. Yield was 11 kg/ha."
    elif index == 29:
        text = ("Calibration, recruitment, and retention procedures were documented; no outcome was stated. " * 120) + " " + statements[0]
    add("candidate_extraction", {"text": text, "maximum_candidates": 8},
        {"mandatory_spans": statements, "candidate_quotes": statements, "allow_empty": not statements})

counts = {}
for row in rows:
    counts[row["task"]] = counts.get(row["task"], 0) + 1
required = {"query_rewrite": 25, "candidate_extraction": 30, "tagging": 20,
            "short_summary": 20, "context_compression": 20, "classification": 25}
assert counts == required, (len(rows), counts)
Path("eval/real-micro-eval.jsonl").write_text(
    "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows),
    encoding="utf-8")
print(f"wrote {len(rows)} fixtures: {counts}")
