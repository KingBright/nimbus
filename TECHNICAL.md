# Nimbus (灵霓) 技术实现与渲染架构设计

Nimbus 桌宠的渲染与交互系统由 Rust 编写，采用 **Wgpu 硬件加速合成视口 + TinySkia 像素分支绘制 + CosmicText 文本排版 + Llama.cpp 绑定引擎**。本文档详细阐述其核心的图形数学公式、状态融合插值以及环境交互逻辑。

---

## 📐 1. 双二次贝塞尔眼睑模型 (Bezier Eyelid Model)

为了让桌宠的眼睛能够从圆润的睁大状态平滑过渡到切平的死鱼眼、月牙笑眼或闭眼睫毛，我们设计了一个双贝塞尔曲线构成的眼睑形状发生器。

对于位于局部坐标系中心 $(cx, cy)$、半宽 $w_{half} = w/2$、半高 $h_{half} = h/2$ 的眼睛，其左右眼角端点为：
- 左眼角 $A = (cx - w_{half}, cy)$
- 右眼角 $B = (cx + w_{half}, cy)$

我们引入了两个控制因子 $top\_lid$ 和 $bottom\_lid$（值域一般在 $[-1.2, 1.2]$）。上眼睑和下眼睑的贝塞尔控制点 $C_{top}$ 和 $C_{bottom}$ 计算公式如下：
$$C_{top} = (cx, cy + h_{half} \times top\_lid)$$
$$C_{bottom} = (cx, cy + h_{half} \times bottom\_lid)$$

### 绘制路径生成
眼睛的边界由两条二次贝塞尔曲线围成，路径生成过程为：
1. 起点移动至左眼角：`PathBuilder::move_to(A.x, A.y)`
2. 绘制上眼睑曲线至右眼角，控制点为 $C_{top}$：`PathBuilder::quad_to(C_top.x, C_top.y, B.x, B.y)`
3. 绘制下眼睑曲线回左眼角，控制点为 $C_{bottom}$：`PathBuilder::quad_to(C_bottom.x, C_bottom.y, A.x, A.y)`
4. 关闭路径并生成。

### 参数映射示例
通过控制这两个因子的正负与大小，可以在同一个路径生成器中变幻出截然不同的眼神：
- **睁眼杏仁眼** ($top\_lid = -1.0, bottom\_lid = 1.0$)：上控制点在 $cy$ 上方，下控制点在 $cy$ 下方，形成上下鼓起的对称眼睛。
- **眯眼月牙笑** ($top\_lid = -0.45, bottom\_lid = -0.4$)：上控制点在上方，下控制点**也在上方**，两条曲线都往上弯，包夹成弯月牙。
- **闭眼静休睫毛** ($top\_lid = 0.4, bottom\_lid = 0.42$)：上控制点和下控制点**都落在 $cy$ 下方**，形成向下微弯的静谧闭眼线。
- **平顶死鱼眼** ($top\_lid = -0.1, bottom\_lid = 1.0$)：上控制点几乎与眼角基线重合，上部拉平，下部保持圆弧。

---

## 🎨 2. 14维参数化混合变形 (Blend-Shape) 缓动

为了防止状态切换时面部特征产生“闪变”或跳变，我们定义了一个含有 14 个连续浮点参数的特征向量：
$$\mathbf{S} = [e_h, e_w, l_{top}, l_{bottom}, p_s, p_o, m_o, m_w, m_s, b_x, b_y, s_a, s_b, g_a]^T$$

参数代表的具体维度如下：
1. $e_h$ / $e_w$：眼睛高度、宽度缩放。
2. $l_{top}$ / $l_{bottom}$：上眼睑、下眼睑贝塞尔因子。
3. $p_s$ / $p_o$：眼珠（瞳孔）缩放大小、眼珠不透明度。
4. $m_o$ / $m_w$ / $m_s$：嘴巴张开度、嘴巴宽度缩放、嘴型歪斜偏度。
5. $b_x$ / $b_y$：身体 X 轴、Y 轴呼吸缩放。
6. $s_a$：身体震颤抖动幅度。
7. $s_b$：睡觉泡泡与 Z 气泡的整体不透明度。
8. $g_a$：全局眼神跟随（盯视）强度。

### 指数平滑更新公式
在每一帧的渲染循环中，系统根据当前的 `pet_state` 查找该状态的**目标参数向量** $\mathbf{S}_{target}$。利用前一帧至今的物理时间差 $dt$，执行与帧率无关的指数缓动计算：
$$\mathbf{S}_{curr} \leftarrow \mathbf{S}_{curr} + (\mathbf{S}_{target} - \mathbf{S}_{curr}) \times (1 - e^{-dt \times 9.5})$$
这使得表情在状态切换时（例如从受惊瞪大到闭眼睡觉），所有几何曲线都在 $\sim 0.15$ 秒内进行连续、圆滑的变形，视觉效果极其流畅自然。

---

## 👁️ 3. 眼神跟随与全局盯视映射

桌宠需要实时盯视鼠标。为了实现“全屏幕跟随”，我们需要将获取到的屏幕全局坐标映射到桌宠的局部眼睑空间内。

### 坐标空间转换
设屏幕绝对鼠标坐标为 $M_{screen} = (x, y)$，桌宠窗口在屏幕上的绝对物理位置为 $W_{pos} = (wx, wy)$，窗口逻辑缩放因子为 $scale\_factor$。
1. **转换窗口内相对物理坐标**：
   $$M_{local\_phys} = (x \times scale\_factor - wx, y \times scale\_factor - wy)$$
2. **投影至局部坐标因子** (归一化至 $[-1.0, 1.0]$)：
   $$P = \left( \frac{M_{local\_phys}.x}{win\_width / 24.0}, \frac{M_{local\_phys}.y}{win\_height / 24.0} \right)$$
3. **眼珠盯视偏移计算**：
   在 `draw_eye` 中，计算眼睛相对于当前坐标变换 $Transform$ 的相对距离，并进行双曲正切饱和截断，防止眼珠转出眼睑外：
   $$dist = \|M_{local\_phys} - Eye_{screen\_pos}\|$$
   $$sat = \tanh\left(\frac{dist}{6.0}\right)$$
   $$Iris\_offset = \left( \frac{dx}{dist} \times sat \times 0.5 \times g_a, \ \frac{dy}{dist} \times sat \times 0.5 \times g_a \right)$$
   通过乘以眼神跟随度因子 $g_a$，使不同情绪状态下的桌宠表现出不同的注意力集中度。

---

## 🧠 4. 模型行为层与稳定状态机

系统先把启动、触摸、自言自语、梦话、形态变化等输入建模为 `BehaviorEvent`。事件 prompt 会结合当前形态、人格、天气、应用、成长记忆和近期表达；不合格输出最多进行两次不携带完整旧 prompt 的短事件重试，避免 1B 模型重复或沉默。

视觉状态由独立的稳定状态机决定。候选状态必须持续达到对应延迟后才提交，因此单帧命中噪声不会让宠物在睡眠、触摸和待机之间跳变。模型 context 是否驻留只属于资源状态，不再控制宠物睡眠。

```rust
let target = if surprised_timer.active() {
    PetState::Surprised
} else if rendered_body_mask.contains(global_pointer) {
    PetState::Tickled
} else if active_event == Dream {
    PetState::Sleeping
} else if model_generation.active() {
    model_generation.is_waiting()
        ? PetState::Thinking
        : PetState::Muttering
} else if user_idle > 15 minutes {
    PetState::Sleeping
} else if user_idle > 90 seconds {
    PetState::Bored
} else if active_app.is_development_tool() {
    PetState::Working
} else if night_time(23:00 - 05:00) {
    PetState::Tired
} else {
    PetState::Idle
};

state = state_machine.commit_after_delay(target);
```

短行为推理使用 512-token context、512-token batch 和 128-token micro-batch。实测 MiniCPM5 1B 的短句 KV Cache 为 12 MiB、Metal 计算缓冲约 64.5 MiB；模型权重本身约 651 MiB，会继续常驻映射以避免每次互动重新加载。

主窗口把配置尺寸视为宠物的名义视觉尺寸，而不是透明窗口宽度。普通形态使用约 `88% × 50%` 的紧凑窗口，云朵为睡眠特效保留少量额外高度；渲染倍率同步补偿，因此收紧窗口不会缩小宠物。气泡窗口从 cosmic-text 的 `line_top + line_height` 计算真实行盒，再加入必要的文字内边距、尾巴和阴影安全区，避免基线重复计入高度造成底部空白。
