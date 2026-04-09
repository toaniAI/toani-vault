# 我的 Bug 待处理清单

- 生成时间: 2026-04-09 21:42:47 (Asia/Shanghai)
- 禅道账号: jiangjiyu
- 产品: zkMe (#25)
- Bug 总数: 13
- 数据来源: ZenTao API v1 `GET /products/25/bugs`，按 `assignedTo=jiangjiyu` 且 `status!=closed` 过滤
- 维护方式: 将每条前面的 `[ ]` 改为 `[1]`表示给出验收标准，改为`[2]`表示已分析原因，改为`[3]`表示已经修复完成待验收，改为`[x]`表示验收通过。

## 清单

- [ ] #18156 【API】凭证管理: 解密凭证，租户A去解密租户B下的凭证 ，状态码应该返回业务错误，而不是500
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T10:41:44Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18156.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18156

- [ ] #18155 【Dashboard】凭证管理:创建的凭证是客户端证书，列表页却返回的是API密钥类型
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P2
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T09:31:34Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18155.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18155

- [ ] #18154 【Dashboard】dashboard:如截图所示的英文应该翻译成中文
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T09:26:04Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18154.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18154

- [ ] #18153 【Dashboard】仪表盘:div被挤压时，里面的文案不应跑到框外
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T09:22:17Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18153.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18153

- [ ] #18152 【Dashboard】租户:请求租户详情接口，接口返回404
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T03:47:47Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18152.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18152

- [ ] #18151 【Dashboard】用户管理:操作字段下”当前用户“不要换行显示
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T02:58:53Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18151.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18151

- [ ] #18150 【Dashboard】租户: 新建租户后，应该停留在租户页，不应该跳转到用户管理页
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P2
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T02:45:42Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18150.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18150

- [ ] #18149 【Dashboard】用户管理: 新用户注册后，应该自动创建默认租户并分配 Owner 角色
  - 产品: zkMe | 状态: active | 严重程度: S2 | 优先级: P2
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T02:39:26Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18149.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18149

- [ ] #18148 【Dashboard】用户管理: 租户邀请成员后，不应该弹这个“你要如何打开此文件”的框
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T01:28:05Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18148.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18148

- [ ] #18147 【Dashboard】租户: 顶部导航栏，应该要显示租户名称，不显示租户ID
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T01:20:40Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18147.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18147

- [ ] #18146 【Dashboard】租户: 租户卡片应该要显示创建时填写的租户名称
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P3
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T01:23:43Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18146.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18146

- [ ] #18143 【Dashboard】Token管理: 无法正常生成token
  - 产品: zkMe | 状态: active | 严重程度: S3 | 优先级: P2
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T01:31:56Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18143.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18143

- [ ] #18140 【Dashboard】登录: 点击连接钱包按钮，应弹出邮箱登录窗口
  - 产品: zkMe | 状态: active | 严重程度: S1 | 优先级: P1
  - 提出人: zhoushichuan | 指派时间: 2026-04-09T01:44:02Z
  - 详情页: https://zt.bitkinetic.com/bug-view-18140.html
  - API详情: https://zt.bitkinetic.com/api.php/v1/bugs/18140
