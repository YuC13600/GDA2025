# 2025通用資料分析 期中提案書

S1154045 王俞捷

uivd978985@gmail.com

## 1. 題目: [齊夫定律(Zipf's law)](https://en.wikipedia.org/wiki/Zipf%27s_law)在影視內容中的實證分析

### 1.1 動機

這個定律是幾年前為了在去歐洲前速成德語發現的，簡單來說在它認為在自然語言中一個單字(詞)出現的頻率與其在頻率表中的排名成反比。

這次我想藉由資料分析驗證其在影視內容中的適用性。

### 1.2 為什麼是影視內容?

相較於日常對話或書籍內容，影視內容是更容易進行大量數據分析的載體。

### 1.3 實際要分析的影視內容類型有哪些?

電影、劇集、動畫、新聞、網路直播等都是這個題目定義下的影視作品。

在這之中電影與劇集因為難以穩定取得大量可分析的內容(正版串流平台的防盜版機制)所以暫時不在考慮的範圍中。

動畫可以透過[ani-cli](https://github.com/pystardust/ani-cli)從allmanga取得足夠的資料，新聞和網路直播則可以透過[yt-dlp](https://github.com/yt-dlp/yt-dlp)從youtube下載可供分析的內容。

新聞的內容包含寫好的稿以及沒有事先撰稿的採訪內容，相較之下動畫是完全由劇本演出而網路直播是完全沒有劇本，適合拿來進行比較。

因此最終決定分析的內容是動畫以及網路直播。

## 2. 方法

### 2.1 資料的取得

如上文所述，主要透過ani-cli與yt-dlp下載網路上的影片內容進行分析

### 2.2 分析前的預處理

#### 2.2.1 影片轉文字

為了在不負擔token費用的前提下處理大量的內容，預計使用OpenAI開源的[whisper](https://github.com/openai/whisper)在我的電腦上進行文字生成而不使用雲端服務。

#### 2.2.2 分詞

如果分析的語言是英語、法語就不需要分詞了，但在動畫中普遍使用日語，所以還是需要將生成的文本分詞後才有辦法統計出現頻率。

因為資料量可能很多，所以預計使用處理速度較快的[MeCab](https://github.com/taku910/mecab)進行分詞，如果精確度太差會改用[GiNZA](https://github.com/megagonlabs/ginza)。

### 2.3 統計

統計時除了有劇本和無劇本的差異，也可以針對不同類型的動畫或直播內容進行更詳細的比較。

> 因為平常習慣用Markdown格式撰寫文件，所以本文的原稿也是用Markdown完成後再轉成word。
> 如果遇到超連結無法點開或其他格式問題可以到[GitHub](https://github.com/YuC13600/GDA2025)瀏覽原稿，其他專案內容也會放上去。


## Reference

齊夫定律(Zipf's law): https://en.wikipedia.org/wiki/Zipf%27s_law

ani-cli: https://github.com/pystardust/ani-cli

yt-dlp: https://github.com/yt-dlp/yt-dlp

whisper: https://github.com/openai/whisper

MeCab: https://github.com/taku910/mecab

GiNZA: https://github.com/megagonlabs/ginza

專案GitHub: https://github.com/YuC13600/GDA2025
