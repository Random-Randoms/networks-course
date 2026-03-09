use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::{
    Deserialize, Serialize,
    ser::{Serializer, SerializeStruct},
};

use std::{
    collections::HashMap,
    sync::Mutex,
};

type ProductId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Product {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IconForm {
    icon: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ProductWithId(ProductId, Product);

#[derive(Debug, Clone, Deserialize)]
struct ProductPatch {
    name: Option<String>,
    description: Option<String>,
}

impl Serialize for ProductWithId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut prd = serializer.serialize_struct("Product", 3)?;
        prd.serialize_field("id", &self.0)?;
        prd.serialize_field("name", &self.1.name)?;
        prd.serialize_field("description", &self.1.description)?;
        prd.end()
    }
}

#[derive(Debug, Clone)]
struct Products {
    products: HashMap<ProductId, Product>,
    images: HashMap<ProductId, Vec<u8>>,
    max_id: ProductId,
}
type SharedProducts = Mutex<Products>;

impl Default for Products {
    fn default() -> Self {
        Self {
            products: HashMap::<ProductId, Product>::new(),
            images: HashMap::<ProductId, Vec<u8>>::new(),
            max_id: 0,
        }
    }
}

impl Products {
    fn push(&mut self, product: Product) -> ProductWithId {
        let id = self.max_id;
        self.max_id += 1;
        self.products.entry(id).insert_entry(product.clone());
        ProductWithId(id, product)
    }

    fn get(&self, id: ProductId) -> Option<Product> {
        self.products.get(&id).cloned()
    }

    fn remove(&mut self, id: ProductId) -> Option<Product> {
        self.products.remove(&id)
    }

    fn put(&mut self, id: ProductId, product: Product) -> Option<Product> {
        match self.products.remove(&id) {
            Some(p) => {
                self.products.insert(id, product);
                Some(p)
            },
            None => None,
        }
    }

    fn products(&self) -> impl Iterator<Item=ProductWithId> {
        self.products.iter().map(|(i, p)| ProductWithId(*i, p.clone()))
    }

    fn set_picture(&mut self, id: ProductId, pic: Vec<u8>) {
        self.images.insert(id, pic);
    }

    fn get_picture(&self, id: ProductId) -> Option<Vec<u8>> {
        self.images.get(&id).cloned()
    }
}

async fn create_product(
    req: web::Json<Product>,
    data: web::Data<SharedProducts>,
) -> impl Responder {
    println!("create product");
    let mut prds = data.lock().unwrap();
    web::Json(prds.push(req.into_inner()))
}

async fn get_product(
    req: web::Path<u64>,
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("get product");
    let id = req.into_inner();
    let prds = data.lock().unwrap();
    match prds.get(id) {
        Some(p) => HttpResponse::Ok().json(ProductWithId(id, p)),
        None => HttpResponse::NotFound().finish(),
    }
}

async fn remove_product(
    req: web::Path<u64>,
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("remove product");
    let id = req.into_inner();
    let mut prds = data.lock().unwrap();
    match prds.remove(id) {
        Some(p) => HttpResponse::Ok().json(ProductWithId(id, p)),
        None => HttpResponse::NotFound().finish(),
    } 
}

async fn all_products(
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("all products");
    let prds = data.lock().unwrap();
    HttpResponse::Ok().json(prds.products().collect::<Vec<_>>())
}

async fn change_product(
    id_path: web::Path<ProductId>,
    req_json: web::Json<ProductPatch>,
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("change product");
    let id = id_path.into_inner();
    let mut prds = data.lock().unwrap();
    match prds.get(id) {
        Some(old) => {
            let ProductPatch { name, description, .. } = req_json.into_inner();
            let new = Product { 
                name: name.unwrap_or(old.name),
                description: description.unwrap_or(old.description)
            };
            let Some(_) = prds.put(id, new.clone()) else { panic!() };
            HttpResponse::Ok().json(ProductWithId(id, new))
        },
        None => HttpResponse::NotFound().finish(),
    }
}

async fn set_picture(
    id_path: web::Path<ProductId>,
    req: web::Bytes,
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("set picture");
    let id = id_path.into_inner();
    let pic = req.to_vec();
    let mut prds = data.lock().unwrap();
    match prds.get(id) {
        Some(_) => {
            prds.set_picture(id, pic);
            HttpResponse::Ok().finish()
        },
        None => {
            HttpResponse::NotFound().finish()
        }
    }
}

async fn get_picture(
    id_path: web::Path<ProductId>,
    data: web::Data<SharedProducts>,
) -> HttpResponse {
    println!("get picture");
    let id = id_path.into_inner();
    let prds = data.lock().unwrap();
    match prds.get_picture(id) {
        Some(pic) => HttpResponse::Ok()
            .content_type("image/png")
            .body(pic),
        None => HttpResponse::NotFound().finish()
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let products = web::Data::new(Mutex::new(Products::default()));
    //env_logger::init_from_env(Env::default().default_filter_or("info"));
    HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(products.clone())
            .service(
                web::scope("/product")
                    .route("/{id}", web::get().to(get_product))
                    .route("/{id}", web::delete().to(remove_product))
                    .route("/{id}", web::put().to(change_product))
                    .route("/", web::post().to(create_product))
                    .route("/{id}/image", web::post().to(set_picture))
                    .route("/{id}/image", web::get().to(get_picture))
            )
            .route("/products", web::get().to(all_products))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[test]
fn test_ser_de() {
    let patch_json = r#"
    {
        "id": 0,
        "name": "aah"
    }
    "#;
    let patch_de: ProductPatch = serde_json::from_str(patch_json).unwrap();
    let patch = ProductPatch {
        id: 0,
        name: "aah",
        description: None,
    };
    assert_eq!(patch_de, patch);
}
