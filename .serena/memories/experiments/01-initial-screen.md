# Initial Astra screen

Decision: preserve baseline and original verifier failure.
Evidence: target/astra-screen-20260907; benchmarks/screening.json and variant-binaries.json. 112 attempted records, no worker failures or watchdog kills. Ten verifier messages on six runs concern preferred witness differences on 24=7+6+5+4+2. Full six-layout sets and saved objects agree; N=7,L=8. Old strict tie selection differed across engines. Later user authorization permits any exact optimal tie; never relabel this original screen as passing.

Two-repeat medians, Custom/Astra cohorts: All min N on 36 60.568/66.976, 115 15.264/62.524, 238 5.168/26.026. 10 One min N/L and 258 All min N/L incomplete at 180. Astra's 258 incumbents: two N9,L14 layouts, first ~116s, not enumeration completion.
Historical Z3: 238 All min N 101.429; 36/115/10/258 incomplete at 180. No universal advantage.
Original runner f6546c55cffadec1f676a46f1be68c4403d62ae8c8334de190b95c1e33d518ca in target/astra-before-partitions-20260908.
