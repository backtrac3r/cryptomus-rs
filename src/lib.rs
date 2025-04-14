use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Client as ReqwestClient, Method};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// --- Константы ---
const CRYPTOMUS_API_BASE_URL: &str = "https://api.cryptomus.com/v1/";
const MERCHANT_HEADER: &str = "merchant";
const SIGN_HEADER: &str = "sign";

pub type CryptomusError = Box<dyn std::error::Error + Send + Sync>;

fn generate_signature(payload_str: &str, api_key: &str) -> Result<String, CryptomusError> {
    if api_key.is_empty() {
        return Err("missing api key".into());
    }
    let encoded_payload = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        payload_str.as_bytes(),
    );
    let data_to_sign = format!("{}{}", encoded_payload, api_key);
    let digest = md5::compute(data_to_sign.as_bytes());
    Ok(format!("{:x}", digest)) // Возвращаем MD5 в виде hex строки
}

// --- Модели данных (Запросы и Ответы) ---

// --- Структуры запросов ---

// Структура для списка разрешенных/исключенных валют
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CurrencyNetwork {
    pub currency: String, // Код валюты
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>, // Код сети (блокчейна)
}

// Запрос на создание счета (invoice)
#[derive(Serialize, Debug, Clone)]
pub struct CreateInvoiceRequest {
    pub amount: String,   // Сумма к оплате (строка, например "10.28")
    pub currency: String, // Код валюты (например, "USD", "USDT", "BTC")
    pub order_id: String, // Уникальный ID заказа в вашей системе
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>, // Код сети (если нужно указать конкретную)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_return: Option<String>, // URL для возврата до оплаты
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_success: Option<String>, // URL для возврата после успешной оплаты
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_callback: Option<String>, // URL для вебхуков
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_payment_multiple: Option<bool>, // Разрешить доплату? (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifetime: Option<i64>, // Время жизни счета в секундах (300-43200, default: 3600)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_currency: Option<String>, // Целевая криптовалюта для конвертации
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtract: Option<i64>, // Процент комиссии, взимаемый с клиента (0-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy_payment_percent: Option<f64>, // Допустимая погрешность оплаты в % (0-5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_data: Option<String>, // Дополнительные данные (до 255 символов)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currencies: Option<Vec<CurrencyNetwork>>, // Список разрешенных валют/сетей
    #[serde(skip_serializing_if = "Option::is_none")]
    pub except_currencies: Option<Vec<CurrencyNetwork>>, // Список исключенных валют/сетей
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course_source: Option<String>, // Источник курса ("Binance", "Kucoin", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_referral_code: Option<String>, // Реферальный код
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_percent: Option<i64>, // Скидка (+) или доп. комиссия (-) в % (-99-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_refresh: Option<bool>, // Обновить истекший счет? (default: false)
}

// Запрос информации о счете
#[derive(Serialize, Debug, Clone)]
pub struct InvoiceInfoRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>, // UUID счета
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>, // ID заказа
}

// --- Структуры ответов ---

// Общая обертка для ответа API Cryptomus
#[derive(Deserialize, Debug, Clone)]
pub struct GenericCryptomusResponse<T> {
    pub state: i64, // 0 - успех, 1 - ошибка
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>, // Данные при успехе
    // Поля при ошибке (state=1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>, // Общее сообщение об ошибке
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>, // Детальные ошибки валидации (может быть объектом)
}

// Статус платежа
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Paid,
    PaidOver,
    WrongAmount,
    Process,
    ConfirmCheck,
    WrongAmountWaiting,
    Check,
    Fail,
    Cancel,
    SystemFail,
    RefundProcess,
    RefundFail,
    RefundPaid,
    Locked,
    // Добавляем неизвестный статус на случай появления новых
    #[serde(other)]
    Unknown,
}

impl PaymentStatus {
    /// .
    ///
    /// # Errors
    ///
    /// This function will return an error if .
    pub fn to_snake_case_string(&self) -> Result<String, serde_json::Error> {
        let json_string = serde_json::to_string(&self)?;
        Ok(json_string.trim_matches('"').to_string())
    }
}

// Структура ответа для счета (invoice)
#[derive(Deserialize, Debug, Clone)]
pub struct InvoiceResponse {
    pub uuid: String,
    pub order_id: String,
    pub amount: String, // Сумма счета в currency
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub payment_amount: Option<String>, // Сколько оплачено клиентом (может быть null)
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub payer_amount: Option<String>, // Сколько должен заплатить клиент в payer_currency
    pub discount_percent: Option<i64>,
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub discount: Option<String>, // Сумма скидки/наценки в криптовалюте
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub payer_currency: Option<String>, // Валюта, в которой платит клиент
    pub currency: String, // Валюта счета
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub merchant_amount: Option<String>, // Сколько будет зачислено на баланс мерчанта
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub network: Option<String>, // Сеть
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub address: Option<String>, // Адрес для оплаты
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub from: Option<String>, // Адрес отправителя (если известен)
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub txid: Option<String>, // Хеш транзакции
    pub payment_status: PaymentStatus, // Статус платежа (важно!)
    pub url: String,      // URL страницы оплаты Cryptomus
    pub expired_at: i64,  // Timestamp истечения срока действия
    #[serde(alias = "status")] // Поле status дублирует payment_status в ответе
    pub status_alias: PaymentStatus,
    pub is_final: bool, // Финализирован ли счет?
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub additional_data: Option<String>,
    pub created_at: String, // Дата создания (UTC+3)
    pub updated_at: String, // Дата обновления (UTC+3)
    #[serde(deserialize_with = "deserialize_optional_string")]
    pub comments: Option<String>, // Комментарии (редко используется)
}

// Десериализатор для полей, которые могут быть null или пустой строкой, но должны быть Option<String>
fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.filter(|val| !val.is_empty()))
}

// --- Клиент Cryptomus ---

#[derive(Clone)]
pub struct CryptomusClient {
    client: ReqwestClient,
    merchant_id: String,
    api_key: String, // Ключ для ПРИЕМА платежей (Payment API Key)
    base_url: String,
}

impl CryptomusClient {
    /// Создает новый клиент Cryptomus API.
    ///
    /// # Arguments
    ///
    /// * `merchant_id` - UUID вашего мерчанта.
    /// * `api_key` - Payment API Key вашего мерчанта.
    #[must_use]
    pub fn new(merchant_id: String, api_key: String) -> Self {
        CryptomusClient {
            client: ReqwestClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Не удалось создать HTTP клиент"),
            merchant_id,
            api_key,
            base_url: CRYPTOMUS_API_BASE_URL.to_string(),
        }
    }

    /// Устанавливает кастомный базовый URL (для тестирования или прокси).
    #[must_use]
    pub fn set_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    // Внутренний метод для отправки запросов
    async fn send_request<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        payload: &T,
    ) -> Result<R, CryptomusError> {
        let url = format!("{}{}", self.base_url, endpoint);

        let payload_str = serde_json::to_string(payload)?;

        let sign = generate_signature(&payload_str, &self.api_key)?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(MERCHANT_HEADER, HeaderValue::from_str(&self.merchant_id)?);
        headers.insert(SIGN_HEADER, HeaderValue::from_str(&sign)?);

        let response = self
            .client
            .request(Method::POST, url)
            .headers(headers)
            .body(payload_str) // Отправляем строку, т.к. она использовалась для подписи
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("Не удалось прочитать тело ответа при ошибке: {}", e));

            match serde_json::from_str::<GenericCryptomusResponse<()>>(&response_text) {
                Ok(err_resp) => {
                    return Err(err_resp.message.unwrap().into());
                }
                Err(_) => {
                    return Err(response_text.into());
                }
            }
        }

        // Если статус успешный (2xx), читаем тело и десериализуем
        let response_text = response.text().await?;
        // println!("Response Body (Success): {}", response_text); // Для отладки
        let parsed_response: GenericCryptomusResponse<R> = serde_json::from_str(&response_text)?;

        if parsed_response.state == 0 {
            let Some(f) = parsed_response.result else {
                return Err("dfdf".into());
            };
            Ok(f)
        } else {
            Err(parsed_response.message.unwrap().into())
        }
    }

    /// .
    ///
    /// # Errors
    ///
    /// This function will return an error if .
    pub async fn create_invoice(
        &self,
        request: &CreateInvoiceRequest,
    ) -> Result<InvoiceResponse, CryptomusError> {
        self.send_request("payment", request).await
    }

    /// .
    ///
    /// # Errors
    ///
    /// This function will return an error if .
    pub async fn get_invoice_info(
        &self,
        request: &InvoiceInfoRequest,
    ) -> Result<InvoiceResponse, CryptomusError> {
        // Проверка, что хотя бы одно поле заполнено
        if request.uuid.is_none() && request.order_id.is_none() {
            return Err("Необходимо указать uuid или order_id".into());
        }
        self.send_request("payment/info", request).await
    }

    // --- Другие методы API можно добавить здесь по аналогии ---
    // Например, для получения списка услуг, баланса, создания статических кошельков, выплат и т.д.
    // Не забывайте проверять, какой API ключ нужен для каждого типа операций (Payment или Payout).
}

// --- Пример использования ---
// Разместите этот код в main.rs или тестах

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // ВАЖНО: Используйте переменные окружения или другие безопасные способы хранения ключей.
//     let merchant_id = std::env::var("CRYPTOMUS_MERCHANT_ID").expect("Нужно установить CRYPTOMUS_MERCHANT_ID");
//     let payment_api_key = std::env::var("CRYPTOMUS_PAYMENT_API_KEY").expect("Нужно установить CRYPTOMUS_PAYMENT_API_KEY");

//     let client = CryptomusClient::new(merchant_id, payment_api_key);

//     // 1. Создание счета на 10 USD (пользователь выберет криптовалюту на странице оплаты)
//     println!("Создание счета на 10 USD...");
//     let order_id = format!("sdk-test-{}", chrono::Utc::now().timestamp()); // Генерируем уникальный ID
//     let invoice_request = CreateInvoiceRequest {
//         amount: "10.00".to_string(),
//         currency: "USD".to_string(),
//         order_id: order_id.clone(),
//         url_callback: Some("https://ваша_ссылка_для_вебхуков.com/callback".to_string()), // Обязательно укажите URL для вебхуков!
//         url_success: Some("https://www.example.com/success".to_string()),
//         lifetime: Some(3600), // 1 час
//         // Остальные параметры по умолчанию (None)
//         network: None,
//         url_return: None,
//         is_payment_multiple: None,
//         to_currency: None,
//         subtract: None,
//         accuracy_payment_percent: None,
//         additional_data: Some("Тестовый счет из Rust SDK".to_string()),
//         currencies: None,
//         except_currencies: None,
//         course_source: None,
//         from_referral_code: None,
//         discount_percent: None,
//         is_refresh: None,
//     };

//     let created_invoice: InvoiceResponse; // Объявляем переменную здесь

//     match client.create_invoice(&invoice_request).await {
//         Ok(invoice) => {
//             println!("Счет успешно создан:");
//             println!("  UUID: {}", invoice.uuid);
//             println!("  Order ID: {}", invoice.order_id);
//             println!("  URL для оплаты: {}", invoice.url);
//             println!("  Статус: {:?}", invoice.payment_status);
//             created_invoice = invoice; // Присваиваем значение
//             // --- Здесь вы перенаправляете пользователя на invoice.url ---
//         }
//         Err(e) => {
//             eprintln!("Ошибка при создании счета: {}", e);
//             // Печать деталей ошибки API, если они есть
//             if let CryptomusError::ApiError { state: _, message, errors } = e {
//                 eprintln!("  Сообщение API: {:?}", message);
//                 eprintln!("  Ошибки валидации API: {:?}", errors);
//             }
//             return Ok(()); // Завершаем программу при ошибке создания
//         }
//     }

//     // Пауза для имитации времени перед проверкой статуса
//     println!("\nОжидание 10 секунд перед проверкой статуса...");
//     tokio::time::sleep(Duration::from_secs(10)).await;

//     // 2. Получение информации о созданном счете по Order ID
//     println!("Получение информации о счете по Order ID: {}...", order_id);
//     let info_request = InvoiceInfoRequest {
//         uuid: None,
//         order_id: Some(order_id.clone()),
//     };

//     match client.get_invoice_info(&info_request).await {
//         Ok(invoice_info) => {
//             println!("Информация о счете получена:");
//             println!("  UUID: {}", invoice_info.uuid);
//             println!("  Order ID: {}", invoice_info.order_id);
//             println!("  Текущий статус: {:?}", invoice_info.payment_status);
//             println!("  Оплачено: {:?}", invoice_info.payment_amount);
//             println!("  Финализирован: {}", invoice_info.is_final);
//             // Можно распечатать больше деталей:
//             // println!("{:#?}", invoice_info);
//         }
//         Err(e) => {
//             eprintln!("Ошибка при получении информации о счете: {}", e);
//             if let CryptomusError::ApiError { state: _, message, errors } = e {
//                 eprintln!("  Сообщение API: {:?}", message);
//                 eprintln!("  Ошибки валидации API: {:?}", errors);
//             }
//         }
//     }

//      // 3. Получение информации о счете по UUID (если есть)
//     println!("\nПолучение информации о счете по UUID: {}...", created_invoice.uuid);
//     let info_request_uuid = InvoiceInfoRequest {
//         uuid: Some(created_invoice.uuid.clone()),
//         order_id: None,
//     };
//      match client.get_invoice_info(&info_request_uuid).await {
//         Ok(invoice_info) => {
//             println!("Информация о счете (по UUID) получена:");
//             println!("  UUID: {}", invoice_info.uuid);
//             println!("  Order ID: {}", invoice_info.order_id);
//             println!("  Текущий статус: {:?}", invoice_info.payment_status);
//         }
//         Err(e) => {
//             eprintln!("Ошибка при получении информации о счете (по UUID): {}", e);
//         }
//     }

//     Ok(())
// }
