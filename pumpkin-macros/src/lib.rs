use heck::{ToPascalCase, ToSnakeCase};
use proc_macro::TokenStream;
use proc_macro_error2::{abort, abort_call_site, proc_macro_error};
use quote::quote;
use syn::spanned::Spanned;
use syn::{self, Attribute, Type};
use syn::{
    Block, Expr, Field, Fields, ItemStruct, Stmt,
    parse::{Nothing, Parser},
    parse_macro_input,
};

#[proc_macro_derive(Event)]
pub fn event(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;

    quote! {
        impl crate::plugin::Event for #name {
            fn get_name_static() -> &'static str {
                stringify!(#name)
            }

            fn get_name(&self) -> &'static str {
                stringify!(#name)
            }

            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn cancellable(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_struct = parse_macro_input!(input as ItemStruct);
    let name = item_struct.ident.clone();
    let _ = parse_macro_input!(args as Nothing);

    if let Fields::Named(ref mut fields) = item_struct.fields {
        fields.named.push(
            Field::parse_named
                .parse2(quote! {
                    /// A boolean indicating cancel state of the event.
                    pub cancelled: bool
                })
                .unwrap(),
        );
    }

    quote! {
        #item_struct

        impl crate::plugin::Cancellable for #name {
            fn cancelled(&self) -> bool {
                self.cancelled
            }

            fn set_cancelled(&mut self, cancelled: bool) {
                self.cancelled = cancelled;
            }
        }
    }
    .into()
}

#[proc_macro_error]
#[proc_macro]
pub fn send_cancellable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Block);

    let mut event = None;
    let mut after_block = None;
    let mut cancelled_block = None;

    for stmt in input.stmts {
        if let Stmt::Expr(expr, _) = stmt {
            if event.is_none() {
                event = Some(expr);
            } else if let Expr::Block(b) = expr {
                if let Some(ref label) = b.label {
                    if label.name.ident == "after" {
                        after_block = Some(b);
                    } else if label.name.ident == "cancelled" {
                        cancelled_block = Some(b);
                    }
                }
            }
        }
    }

    if let Some(event) = event {
        if let Some(after_block) = after_block {
            if let Some(cancelled_block) = cancelled_block {
                quote! {
                    let event = crate::PLUGIN_MANAGER
                        .fire(#event)
                        .await;

                    if !event.cancelled {
                        #after_block
                    } else {
                        #cancelled_block
                    }
                }
                .into()
            } else {
                quote! {
                    let event = crate::PLUGIN_MANAGER
                        .fire(#event)
                        .await;

                    if !event.cancelled {
                        #after_block
                    }
                }
                .into()
            }
        } else if let Some(cancelled_block) = cancelled_block {
            quote! {
                let event = crate::PLUGIN_MANAGER
                    .fire(#event)
                    .await;

                if event.cancelled {
                    #cancelled_block
                }
            }
            .into()
        } else {
            quote! {
                let event = crate::PLUGIN_MANAGER
                    .fire(#event)
                    .await;
            }
            .into()
        }
    } else {
        abort_call_site!("Event must be specified");
    }
}

#[proc_macro_attribute]
pub fn packet(input: TokenStream, item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item.clone()).unwrap();
    let name = &ast.ident;
    let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();

    let input: proc_macro2::TokenStream = input.into();
    let item: proc_macro2::TokenStream = item.into();

    let code = quote! {
        #item
        impl #impl_generics crate::packet::Packet for #name #ty_generics {
            const PACKET_ID: i32 = #input;
        }
    };

    code.into()
}

#[proc_macro_attribute]
pub fn pumpkin_block(input: TokenStream, item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item.clone()).unwrap();
    let name = &ast.ident;
    let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();

    let input_string = input.to_string();
    let packet_name = input_string.trim_matches('"');
    let (namespace, id) = packet_name
        .split_once(":")
        .unwrap_or_else(|| abort!(packet_name, "A namespace is required!"));

    let item: proc_macro2::TokenStream = item.into();

    let code = quote! {
        #item
        impl #impl_generics crate::block::BlockMetadata for #name #ty_generics {
            fn namespace(&self) -> &'static str {
                #namespace
            }
            fn ids(&self) -> &'static [&'static str] {
                &[#id]
            }
        }
    };

    code.into()
}

#[proc_macro_attribute]
pub fn pumpkin_block_from_tag(input: TokenStream, item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item.clone()).unwrap();
    let name = &ast.ident;
    let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();

    let input_string = input.to_string();
    let packet_name = input_string.trim_matches('"');
    let packet_name_split: Vec<&str> = packet_name.split(":").collect();

    let namespace = packet_name_split[0];

    let item: proc_macro2::TokenStream = item.into();

    let code = quote! {
        #item
        impl #impl_generics crate::block::BlockMetadata for #name #ty_generics {
            fn namespace(&self) -> &'static str {
                #namespace
            }
            fn ids(&self) -> &'static [&'static str] {
                get_tag_values(RegistryKey::Block, #packet_name).unwrap()
            }
        }
    };

    code.into()
}

#[proc_macro_error]
#[proc_macro_attribute]
pub fn block_property(input: TokenStream, item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item.clone()).unwrap();
    let name = &ast.ident;
    let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();

    let input_string = input.to_string();
    let input_parts: Vec<&str> = input_string.split("[").collect();
    let property_name = input_parts[0].trim_ascii().trim_matches(&['"', ','][..]);
    let mut property_values: Vec<&str> = Vec::new();
    if input_parts.len() > 1 {
        property_values = input_parts[1]
            .trim_matches(']')
            .split(", ")
            .map(|p| p.trim_ascii().trim_matches(&['"', ','][..]))
            .collect::<Vec<&str>>();
    }

    let item: proc_macro2::TokenStream = item.into();

    let (variants, is_enum): (Vec<proc_macro2::Ident>, bool) = match ast.data {
        syn::Data::Enum(enum_item) => (
            enum_item.variants.into_iter().map(|v| v.ident).collect(),
            true,
        ),
        syn::Data::Struct(s) => {
            let fields = match s.fields {
                Fields::Named(f) => abort!(f.span(), "Block properties can't have named fields"),
                Fields::Unnamed(fields) => fields.unnamed,
                Fields::Unit => abort!(s.fields.span(), "Block properties must have fields"),
            };
            if fields.len() != 1 {
                abort!(
                    fields.span(),
                    "Block properties `struct`s must have exactly one field"
                );
            }
            let field = fields.first().unwrap();
            let ty = &field.ty;
            let struct_type = match field.ty {
                syn::Type::Path(ref type_path) => {
                    type_path.path.segments.first().unwrap().ident.to_string()
                }
                ref other => abort!(
                    other.span(),
                    "Block properties can only have primitive types"
                ),
            };
            match struct_type.as_str() {
                "bool" => (
                    vec![
                        proc_macro2::Ident::new("true", proc_macro2::Span::call_site()),
                        proc_macro2::Ident::new("false", proc_macro2::Span::call_site()),
                    ],
                    false,
                ),
                other => abort!(
                    ty.span(),
                    format!("`{other}` is not supported (why not implement it yourself?)")
                ),
            }
        }
        _ => abort_call_site!("Block properties can only be `enum`s or `struct`s"),
    };

    let values = variants.iter().enumerate().map(|(i, v)| match is_enum {
        true => {
            let mut value = v.to_string().to_snake_case();
            if !property_values.is_empty() && i < property_values.len() {
                value = property_values[i].to_string();
            }
            quote! {
                Self::#v => #value.to_string(),
            }
        }
        false => {
            let value = v.to_string();
            quote! {
                Self(#v) => #value.to_string(),
            }
        }
    });

    let from_values = variants.iter().enumerate().map(|(i, v)| match is_enum {
        true => {
            let mut value = v.to_string().to_snake_case();
            if !property_values.is_empty() && i < property_values.len() {
                value = property_values[i].to_string();
            }
            quote! {
                #value => Self::#v,
            }
        }
        false => {
            let value = v.to_string();
            quote! {
                #value => Self(#v),
            }
        }
    });

    let extra_fns = variants.iter().map(|v| {
        let title = proc_macro2::Ident::new(
            &v.to_string().to_pascal_case(),
            proc_macro2::Span::call_site(),
        );
        quote! {
            pub fn #title() -> Self {
                Self(#v)
            }
        }
    });

    let extra = if is_enum {
        quote! {}
    } else {
        quote! {
            impl #name {
                #(#extra_fns)*
            }
        }
    };

    let code = quote! {
        #item
        impl #impl_generics pumpkin_world::block::properties::BlockPropertyMetadata for #name #ty_generics {
            fn name(&self) -> &'static str {
                #property_name
            }
            fn value(&self) -> String {
                match self {
                    #(#values)*
                }
            }
            fn from_value(value: String) -> Self {
                match value.as_str() {
                    #(#from_values)*
                    _ => panic!("Invalid value for block property"),
                }
            }
        }
        #extra
    };

    code.into()
}

#[rustfmt::skip]
#[proc_macro_derive(PacketWrite, attributes(serial))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;

    let fields = if let syn::Data::Struct(data) = &input.data {
        data.fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let (is_big_endian, no_prefix) = check_serial_attributes(&f.attrs);
            let is_vec = is_vec(&f.ty);

            if is_vec && !no_prefix {
                // Vec with prefix: write VarUInt length, then data
                if is_big_endian {
                    quote! {
                        crate::codec::var_uint::VarUInt(self.#ident.len() as u32).write(writer)?;
                        self.#ident.write_be(writer)?;
                    }
                } else {
                    quote! {
                        crate::codec::var_uint::VarUInt(self.#ident.len() as u32).write(writer)?;
                        self.#ident.write(writer)?;
                    }
                }
            } else {
                // Non-Vec or Vec with no_prefix: write directly
                if is_big_endian {
                    quote! {
                        self.#ident.write_be(writer)?;
                    }
                } else {
                    quote! {
                        self.#ident.write(writer)?;
                    }
                }
            }
        })
    } else {
        unimplemented!()
    };

    let expanded = quote! {
        impl PacketWrite for #name {
            fn write<W: std::io::Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
                #(#fields)*
                Ok(())
            }
        }
    };

    expanded.into()
}

#[rustfmt::skip]
#[proc_macro_derive(PacketRead, attributes(serial))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;

    let fields = if let syn::Data::Struct(data) = &input.data {
        data.fields.iter().map(|f| {
            let ident = f.ident.as_ref().unwrap();
            let (is_big_endian, no_prefix) = check_serial_attributes(&f.attrs);
            let is_vec = is_vec(&f.ty);

            if is_vec && !no_prefix {
                // Vec with prefix: read VarUInt length, then data
                quote! {
                    #ident: {
                        let len = crate::codec::var_uint::VarUInt::read(reader)?.0 as usize;
                        let mut buf = vec![0u8; len];
                        reader.read_exact(&mut buf)?;
                        buf
                    }
                }
            } else {
                // Non-Vec or Vec with no_prefix: read directly
                if is_big_endian {
                    quote! {
                        #ident: PacketRead::read_be(reader)?
                    }
                } else {
                    quote! {
                        #ident: PacketRead::read(reader)?
                    }
                }
            }
        })
    } else {
        unimplemented!()
    };

    let expanded = quote! {
        impl PacketRead for #name {
            fn read<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                Ok(Self {
                    #(#fields),*
                })
            }
        }
    };

    expanded.into()
}

fn check_serial_attributes(attrs: &[Attribute]) -> (bool, bool) {
    let mut is_big_endian = false;
    let mut no_prefix = false;

    for attr in attrs.iter() {
        if attr.path().is_ident("serial") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("big_endian") {
                    is_big_endian = true;
                } else if meta.path.is_ident("no_prefix") {
                    no_prefix = true;
                }
                Ok(())
            });
        }
    }

    (is_big_endian, no_prefix)
}

fn is_vec(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        type_path
            .path
            .segments
            .iter()
            .last()
            .map(|segment| segment.ident == "Vec")
            .unwrap_or(false)
    } else {
        false
    }
}
